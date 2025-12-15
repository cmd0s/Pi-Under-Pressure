use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tokio::sync::mpsc;

use pi_under_pressure::{
    detection,
    stress::{self, StressConfig},
    system,
    ui::{self, UiMode},
};

#[derive(Parser, Debug)]
#[command(name = "pi-under-pressure")]
#[command(author = "Pi Under Pressure Contributors")]
#[command(version)]
#[command(about = "Stability tester for overclocked Raspberry Pi 5")]
#[command(after_help = "NOTE: Run with sudo for full functionality (hardware sensors, NVMe stress testing).\n\nExample: sudo pi-under-pressure --duration 1h")]
struct Args {
    /// Test duration (e.g., 30m, 1h, 2h30m)
    #[arg(short, long, default_value = "30m")]
    duration: String,

    /// Force extended mode (include NVMe stress)
    #[arg(short, long)]
    extended: bool,

    /// Enable hardware video encoder stress
    #[arg(long)]
    video: bool,

    /// Test only CPU (skip RAM and NVMe)
    #[arg(short = 'c', long)]
    cpu_only: bool,

    /// Test only RAM
    #[arg(short = 'm', long)]
    memory_only: bool,

    /// Test only NVMe
    #[arg(short = 'n', long)]
    nvme_only: bool,

    /// Custom path for NVMe stress test file
    #[arg(long)]
    nvme_path: Option<String>,

    /// Number of CPU threads [default: all cores]
    #[arg(short, long)]
    threads: Option<usize>,

    /// Status update interval in seconds
    #[arg(short, long, default_value = "2")]
    interval: u64,

    /// Use simple output instead of TUI
    #[arg(long)]
    simple: bool,

    /// Disable colors
    #[arg(long)]
    no_color: bool,

    /// Output final report in JSON format
    #[arg(long)]
    json: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("Invalid duration '{}': {}", s, e))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Parse duration
    let duration = parse_duration(&args.duration)?;

    // Setup shutdown signal
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Handle Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        r.store(false, Ordering::SeqCst);
    });

    // Collect system information
    let sys_info = system::info::collect_system_info();
    let oc_config = system::config::parse_config();
    let nvme_info = detection::nvme::detect_nvme();

    // Determine UI mode
    let ui_mode = if args.simple {
        UiMode::Simple
    } else {
        UiMode::Tui
    };

    // Determine what to test
    // NVMe stress only runs with --extended or --nvme-only flags (not auto-detected)
    let stress_config = StressConfig {
        cpu: !args.memory_only && !args.nvme_only,
        memory: !args.cpu_only && !args.nvme_only,
        nvme: (args.extended || args.nvme_only) && nvme_info.is_some() && !args.cpu_only && !args.memory_only,
        video: args.video,
        threads: args.threads.unwrap_or_else(num_cpus),
        duration,
        nvme_path: args.nvme_path,
    };

    // Create channels for communication
    let (stats_tx, stats_rx) = mpsc::channel(100);
    let (event_tx, _event_rx) = mpsc::channel::<String>(100);

    // Display system info
    ui::display_system_info(&sys_info, &oc_config, &nvme_info, ui_mode, args.no_color);

    // Start the UI
    let ui_handle = if ui_mode == UiMode::Tui {
        Some(tokio::spawn(ui::tui::run_tui(
            stats_rx,
            running.clone(),
            duration,
            args.interval,
        )))
    } else {
        None
    };

    // Run stress test
    let test_result = stress::run_stress_test(
        stress_config.clone(),
        running.clone(),
        stats_tx,
        event_tx,
        nvme_info.clone(),
    )
    .await;

    // Wait for UI to finish
    if let Some(handle) = ui_handle {
        handle.await.ok();
    }

    // Collect final errors from dmesg
    let io_errors = detection::errors::check_dmesg_errors();

    // Generate and display final report
    let report = stress::generate_report(&test_result, &io_errors, duration);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        ui::display_final_report(&report, args.no_color);
    }

    // Exit with appropriate code
    if report.passed {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}
