use crate::detection::nvme::NvmeInfo;
use crate::stress::{FinalReport, StressStats};
use crate::system::config::OcConfig;
use crate::system::info::SystemInfo;

use super::format_duration;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const DIM: &str = "\x1b[2m";

/// Display system information in simple text format
pub fn display_system_info(
    sys_info: &SystemInfo,
    oc_config: &OcConfig,
    nvme_info: &Option<NvmeInfo>,
    no_color: bool,
) {
    let (bold, reset, cyan, dim) = if no_color {
        ("", "", "", "")
    } else {
        (BOLD, RESET, CYAN, DIM)
    };

    println!();
    println!(
        "{}┌──────────────────────────────────────────────────────────────{}",
        cyan, reset
    );
    println!(
        "{}│  {}Pi Under Pressure{} v{}{}",
        cyan,
        bold,
        reset,
        env!("CARGO_PKG_VERSION"),
        reset
    );
    println!(
        "{}├──────────────────────────────────────────────────────────────{}",
        cyan, reset
    );

    // System section
    println!("{}│  {}SYSTEM{}{}", cyan, bold, reset, reset);
    println!(
        "{}│  Model:          {}{}",
        cyan, sys_info.model, reset
    );
    println!(
        "{}│  Serial:         {}{}",
        cyan, sys_info.serial, reset
    );
    println!(
        "{}│  Firmware:       {}{}",
        cyan,
        truncate_str(&sys_info.firmware, 50),
        reset
    );
    println!(
        "{}│  CPU:            {} ({} cores){}",
        cyan,
        sys_info.cpu,
        sys_info.cpu_cores,
        reset
    );
    println!(
        "{}│  RAM:            {} MB{}",
        cyan, sys_info.ram_mb, reset
    );
    println!(
        "{}│  OS:             {}{}",
        cyan,
        truncate_str(&sys_info.os, 50),
        reset
    );
    println!(
        "{}│  Kernel:         {}{}",
        cyan, sys_info.kernel, reset
    );

    // Overclocking section
    println!(
        "{}├──────────────────────────────────────────────────────────────{}",
        cyan, reset
    );
    println!("{}│  {}OVERCLOCKING{} (/boot/firmware/config.txt){}", cyan, bold, reset, reset);

    let mut has_oc_params = false;

    if let Some(freq) = oc_config.arm_freq {
        has_oc_params = true;
        let default_note = if freq > 2400 {
            format!(" {}(default: 2400){}", dim, reset)
        } else {
            String::new()
        };
        println!(
            "{}│  arm_freq:           {} MHz{}{}",
            cyan, freq, default_note, reset
        );
    }

    if let Some(freq) = oc_config.gpu_freq {
        has_oc_params = true;
        let default_note = if freq > 910 {
            format!(" {}(default: 910){}", dim, reset)
        } else {
            String::new()
        };
        println!(
            "{}│  gpu_freq:           {} MHz{}{}",
            cyan, freq, default_note, reset
        );
    }

    if let Some(delta) = oc_config.over_voltage_delta {
        has_oc_params = true;
        let mv = delta as f32 / 1000.0;
        println!(
            "{}│  over_voltage_delta: {} µV (+{:.2}mV){}",
            cyan, delta, mv, reset
        );
    }

    if let Some(ov) = oc_config.over_voltage {
        has_oc_params = true;
        println!(
            "{}│  over_voltage:       {}{}",
            cyan, ov, reset
        );
    }

    if let Some(ft) = oc_config.force_turbo {
        has_oc_params = true;
        println!(
            "{}│  force_turbo:        {}{}",
            cyan, ft, reset
        );
    }

    if let Some(gen) = oc_config.pcie_gen {
        has_oc_params = true;
        println!(
            "{}│  dtparam=pciex1_gen: {}{}",
            cyan, gen, reset
        );
    }

    if !has_oc_params {
        println!("{}│  {}(no overclocking parameters set){}",
            cyan, dim, reset
        );
    }

    // Runtime section
    println!(
        "{}├──────────────────────────────────────────────────────────────{}",
        cyan, reset
    );
    println!("{}│  {}RUNTIME{}{}", cyan, bold, reset, reset);

    let min_freq = crate::system::monitor::get_cpu_freq_min();
    let cur_freq = crate::system::monitor::get_cpu_freq();
    let max_freq = crate::system::monitor::get_cpu_freq_max();
    println!(
        "{}│  Min CPU freq:       {} MHz{}",
        cyan, min_freq, reset
    );
    println!(
        "{}│  Cur CPU freq:       {} MHz{}",
        cyan, cur_freq, reset
    );
    println!(
        "{}│  Max CPU freq:       {} MHz{}",
        cyan, max_freq, reset
    );

    let governor = crate::system::monitor::get_governor();
    println!(
        "{}│  CPU Governor:       {}{}",
        cyan, governor, reset
    );

    // Storage section
    if let Some(nvme) = nvme_info {
        println!(
            "{}├──────────────────────────────────────────────────────────────{}",
            cyan, reset
        );
        println!("{}│  {}STORAGE{}{}", cyan, bold, reset, reset);
        println!(
            "{}│  NVMe Detected:      {}{}",
            cyan,
            truncate_str(&nvme.model, 45),
            reset
        );

        if let Some(gen) = nvme.pcie_gen {
            let speed = match gen {
                3 => "~900 MB/s",
                2 => "~450 MB/s",
                _ => "~250 MB/s",
            };
            println!(
                "{}│  PCIe Generation:    Gen {}.0 x1 ({}){}",
                cyan, gen, speed, reset
            );
        }

        if let Some(temp) = crate::detection::nvme::get_nvme_temp(&nvme.device_path) {
            println!(
                "{}│  NVMe Temperature:   {:.1}°C{}",
                cyan, temp, reset
            );
        }
    }

    println!(
        "{}└──────────────────────────────────────────────────────────────{}",
        cyan, reset
    );
    println!();
}

/// Display real-time stats in simple format
pub fn display_stats(stats: &StressStats, duration_secs: u64, no_color: bool) {
    let (green, yellow, red, cyan, reset) = if no_color {
        ("", "", "", "", "")
    } else {
        (GREEN, YELLOW, RED, CYAN, RESET)
    };

    let elapsed = format_duration(stats.elapsed_secs);
    let remaining = format_duration(duration_secs.saturating_sub(stats.elapsed_secs));

    // Color temperature based on value
    let temp_color = if stats.cpu_temp_c >= 85.0 {
        red
    } else if stats.cpu_temp_c >= 80.0 {
        yellow
    } else {
        green
    };

    // Throttle indicator
    let throttle_str = if stats.throttle_status.has_any_current_issue() {
        format!("{}YES{}", red, reset)
    } else {
        format!("{}None{}", green, reset)
    };

    // Fan speed
    let fan_str = match (stats.fan_status.speed_percent, stats.fan_status.rpm) {
        (Some(pct), Some(rpm)) => format!("{}{}%({} RPM){}", cyan, pct, rpm, reset),
        (Some(pct), None) => format!("{}{}%{}", cyan, pct, reset),
        (None, Some(rpm)) => format!("{}{} RPM{}", cyan, rpm, reset),
        (None, None) => format!("{}N/A{}", cyan, reset),
    };

    print!(
        "\r[{}] CPU: {}{:.1}°C{} | Freq: {} MHz | Throttle: {} | Fan: {} | RAM: {}/{} MB | {:.0}% | ETA: {}   ",
        elapsed,
        temp_color,
        stats.cpu_temp_c,
        reset,
        stats.cpu_freq_mhz,
        throttle_str,
        fan_str,
        stats.mem_used_mb,
        stats.mem_total_mb,
        stats.progress_percent,
        remaining
    );

    // Flush to ensure it's displayed
    use std::io::{self, Write};
    io::stdout().flush().ok();
}

/// Display final report
pub fn display_final_report(report: &FinalReport, no_color: bool) {
    let (bold, reset, green, red) = if no_color {
        ("", "", "", "")
    } else {
        (BOLD, RESET, GREEN, RED)
    };

    let check = if no_color { "[OK]" } else { "✓" };
    let cross = if no_color { "[FAIL]" } else { "✗" };

    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("{}                    STABILITY TEST RESULTS{}                    ", bold, reset);
    println!("═══════════════════════════════════════════════════════════════");

    println!(
        "Duration:          {}",
        format_duration(report.duration_secs)
    );

    let result_str = if report.passed {
        format!("{}{} PASSED{} (system is stable)", green, check, reset)
    } else {
        format!("{}{} FAILED{} (issues detected)", red, cross, reset)
    };
    println!("Result:            {}", result_str);
    println!();

    println!("Workloads:");
    println!(
        "  CPU Stress:        {} {}",
        if report.cpu_stress_passed {
            format!("{}{}{}", green, check, reset)
        } else {
            format!("{}{}{}", red, cross, reset)
        },
        if report.cpu_stress_passed {
            "No computation errors"
        } else {
            "Computation errors detected"
        }
    );
    println!(
        "  Memory Stress:     {} {}",
        if report.memory_stress_passed {
            format!("{}{}{}", green, check, reset)
        } else {
            format!("{}{}{}", red, cross, reset)
        },
        if report.memory_stress_passed {
            "All patterns verified"
        } else {
            "Memory errors detected"
        }
    );
    println!(
        "  NVMe Stress:       {} {}",
        if report.nvme_stress_passed {
            format!("{}{}{}", green, check, reset)
        } else {
            format!("{}{}{}", red, cross, reset)
        },
        if report.nvme_stress_passed {
            "No I/O errors"
        } else {
            "I/O errors detected"
        }
    );
    println!();

    println!("Temperature Stats:");
    println!("  CPU Max:         {:.1}°C (threshold: 85°C)", report.max_cpu_temp);
    println!("  CPU Avg:         {:.1}°C", report.avg_cpu_temp);
    if let Some(nvme_max) = report.max_nvme_temp {
        println!("  NVMe Max:        {:.1}°C", nvme_max);
    }
    println!();

    println!("Events:");
    let throttle_color = if report.throttle_events > 0 { red } else { green };
    println!(
        "  Throttling:      {}{}{}",
        throttle_color, report.throttle_events, reset
    );

    let voltage_color = if report.under_voltage_events > 0 {
        red
    } else {
        green
    };
    println!(
        "  Under-voltage:   {}{}{}",
        voltage_color, report.under_voltage_events, reset
    );

    let io_color = if report.io_errors > 0 { red } else { green };
    println!("  I/O Errors:      {}{}{}", io_color, report.io_errors, reset);

    let smart_color = if report.smart_warnings > 0 { red } else { green };
    println!(
        "  SMART Warnings:  {}{}{}",
        smart_color, report.smart_warnings, reset
    );

    println!("═══════════════════════════════════════════════════════════════");
    println!();
}

/// Truncate string to max length with ellipsis
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
