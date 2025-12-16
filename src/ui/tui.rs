use std::io::{self, stdout, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use tokio::sync::mpsc;

use crate::stress::StressStats;
use super::format_duration;

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Initialize the terminal for TUI
fn init_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// Restore terminal to normal state
fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Run the TUI event loop
pub async fn run_tui(
    mut stats_rx: mpsc::Receiver<StressStats>,
    running: Arc<AtomicBool>,
    total_duration: Duration,
    update_interval: u64,
) {
    // Initialize terminal
    let mut terminal = match init_terminal() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to initialize TUI: {}", e);
            return;
        }
    };

    let mut current_stats = StressStats::default();
    let total_secs = total_duration.as_secs();

    // Main event loop
    loop {
        // Check if we should stop
        if !running.load(Ordering::Relaxed) {
            break;
        }

        // Handle events with timeout
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            running.store(false, Ordering::SeqCst);
                            break;
                        }
                        KeyCode::Esc => {
                            running.store(false, Ordering::SeqCst);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Try to receive new stats (non-blocking)
        while let Ok(stats) = stats_rx.try_recv() {
            current_stats = stats;
        }

        // Draw the UI
        if let Err(e) = terminal.draw(|frame| {
            render_ui(frame, &current_stats, total_secs);
        }) {
            eprintln!("Failed to draw TUI: {}", e);
            break;
        }
    }

    // Restore terminal
    if let Err(e) = restore_terminal(&mut terminal) {
        eprintln!("Failed to restore terminal: {}", e);
    }
}

/// Render the main UI
fn render_ui(frame: &mut Frame, stats: &StressStats, total_secs: u64) {
    let size = frame.area();

    // Calculate stats height based on number of cores (2 header lines + per-core bars + borders)
    let num_cores = stats.cpu_usage_per_core.len().max(4);
    let stats_height = (2 + num_cores + 2) as u16; // +2 for borders

    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),           // Title
            Constraint::Length(stats_height), // Stats (dynamic based on cores)
            Constraint::Length(5),           // Progress
            Constraint::Length(4),           // Footer (with GitHub link)
        ])
        .split(size);

    // Title
    render_title(frame, chunks[0], stats);

    // Stats
    render_stats(frame, chunks[1], stats);

    // Progress
    render_progress(frame, chunks[2], stats, total_secs);

    // Footer
    render_footer(frame, chunks[3]);
}

fn render_title(frame: &mut Frame, area: Rect, stats: &StressStats) {
    let elapsed = format_duration(stats.elapsed_secs);

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " Pi Under Pressure ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ─ Stability Tester ─ "),
        Span::styled(
            elapsed,
            Style::default().fg(Color::Yellow),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(title, area);
}

fn render_stats(frame: &mut Frame, area: Rect, stats: &StressStats) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left column - CPU stats with per-core usage
    let cpu_temp_color = if stats.cpu_temp_c >= 85.0 {
        Color::Red
    } else if stats.cpu_temp_c >= 80.0 {
        Color::Yellow
    } else if stats.cpu_temp_c >= 70.0 {
        Color::LightYellow
    } else {
        Color::Green
    };

    let throttle_status = if stats.throttle_status.has_any_current_issue() {
        Span::styled("YES - THROTTLING!", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("None", Style::default().fg(Color::Green))
    };

    // Build CPU info lines including per-core usage
    let mut cpu_lines = vec![
        Line::from(vec![
            Span::raw("  Temp: "),
            Span::styled(
                format!("{:.1}°C", stats.cpu_temp_c),
                Style::default().fg(cpu_temp_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" (max: {:.1}°C)", stats.cpu_temp_max),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("  Freq: "),
            Span::styled(
                format!("{} MHz", stats.cpu_freq_mhz),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Throttling: "),
            throttle_status,
            Span::raw("  Errors: "),
            Span::styled(
                format!("{}", stats.cpu_errors),
                Style::default().fg(if stats.cpu_errors > 0 { Color::Red } else { Color::Green }),
            ),
        ]),
    ];

    // Add per-core CPU usage bars
    for (i, usage) in stats.cpu_usage_per_core.iter().enumerate() {
        let bar_width = 12;
        let filled = ((usage / 100.0) * bar_width as f32) as usize;
        let bar_color = if *usage >= 95.0 {
            Color::Green
        } else if *usage >= 50.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        cpu_lines.push(Line::from(vec![
            Span::raw(format!("  CPU{}: [", i)),
            Span::styled(
                "█".repeat(filled.min(bar_width)),
                Style::default().fg(bar_color),
            ),
            Span::styled(
                "░".repeat(bar_width.saturating_sub(filled)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("] "),
            Span::styled(
                format!("{:5.1}%", usage),
                Style::default().fg(bar_color),
            ),
        ]));
    }

    let cpu_info = Paragraph::new(cpu_lines)
        .block(
            Block::default()
                .title(" CPU ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );

    frame.render_widget(cpu_info, chunks[0]);

    // Right column - Memory, NVMe, and Fan stats
    let nvme_temp_str = match stats.nvme_temp_c {
        Some(temp) => format!("{:.1}°C", temp),
        None => "N/A".to_string(),
    };

    // Fan status string
    let fan_str = match (stats.fan_status.speed_percent, stats.fan_status.rpm) {
        (Some(pct), Some(rpm)) => format!("{}% ({} RPM)", pct, rpm),
        (Some(pct), None) => format!("{}%", pct),
        (None, Some(rpm)) => format!("{} RPM", rpm),
        (None, None) => "N/A".to_string(),
    };

    // NVMe test path string
    let nvme_test_path_str = match &stats.nvme_test_path {
        Some(path) => path.clone(),
        None => "N/A".to_string(),
    };

    let mem_info = Paragraph::new(vec![
        Line::from(vec![
            Span::raw("  RAM Usage:        "),
            Span::styled(
                format!("{} / {} MB", stats.mem_used_mb, stats.mem_total_mb),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Fan Speed:        "),
            Span::styled(
                fan_str,
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Memory Errors:    "),
            Span::styled(
                format!("{}", stats.memory_errors),
                Style::default().fg(if stats.memory_errors > 0 { Color::Red } else { Color::Green }),
            ),
        ]),
        Line::from(vec![
            Span::raw("  I/O Errors:       "),
            Span::styled(
                format!("{}", stats.io_errors),
                Style::default().fg(if stats.io_errors > 0 { Color::Red } else { Color::Green }),
            ),
        ]),
        Line::from(vec![
            Span::raw("  NVMe Temperature: "),
            Span::styled(
                nvme_temp_str,
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("  NVMe Test File:   "),
            Span::styled(
                nvme_test_path_str,
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Video Errors:     "),
            Span::styled(
                format!("{}", stats.video_errors),
                Style::default().fg(if stats.video_errors > 0 { Color::Red } else { Color::Green }),
            ),
        ]),
    ])
    .block(
        Block::default()
            .title(" Memory & Storage ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(mem_info, chunks[1]);
}

fn render_progress(frame: &mut Frame, area: Rect, stats: &StressStats, total_secs: u64) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(1)])
        .split(area);

    // Progress bar
    let progress = stats.progress_percent.min(100.0) / 100.0;
    let remaining_secs = total_secs.saturating_sub(stats.elapsed_secs);
    let remaining_str = format_duration(remaining_secs);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray),
        )
        .percent((stats.progress_percent as u16).min(100))
        .label(format!(
            "{:.1}% | Remaining: {}",
            stats.progress_percent, remaining_str
        ));

    frame.render_widget(gauge, chunks[0]);

    // Temperature gauge
    let temp_percent = ((stats.cpu_temp_c / 100.0) * 100.0) as u16;
    let temp_color = if stats.cpu_temp_c >= 85.0 {
        Color::Red
    } else if stats.cpu_temp_c >= 80.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let temp_bar = format!(
        "  CPU Temp: [{}{}] {:.1}°C / 85°C",
        "█".repeat((temp_percent as usize * 30 / 100).min(30)),
        "░".repeat(30 - (temp_percent as usize * 30 / 100).min(30)),
        stats.cpu_temp_c
    );

    let temp_line = Paragraph::new(Line::from(vec![
        Span::styled(temp_bar, Style::default().fg(temp_color)),
    ]));

    frame.render_widget(temp_line, chunks[1]);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                " GitHub: ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "https://github.com/cmd0s/Pi-Under-Pressure",
                Style::default().fg(Color::Blue),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " Press 'q' or Ctrl+C to stop test gracefully ",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(footer, area);
}
