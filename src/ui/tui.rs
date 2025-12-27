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
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use tokio::sync::mpsc;

use super::format_duration;
use crate::stress::StressStats;

/// ASCII art title - "Pi Under Pressure" in Fire Font-s style
const ASCII_TITLE: &[&str] = &[
    r" (                 ) (         (      (   (       (   (         (        ",
    r" )\ )           ( /( )\ )      )\ )   )\ ))\ )    )\ ))\ )      )\ )     ",
    r"(()/((       (  )\()|()/(  (  (()/(  (()/(()/((  (()/(()/(   ( (()/((    ",
    r" /(_))\      )\((_)\ /(_)) )\  /(_))  /(_))(_))\  /(_))(_))  )\ /(_))\   ",
    r"(_))((_)  _ ((_)_((_|_))_ ((_)(_))   (_))(_))((_)(_))(_)) _ ((_|_))((_)  ",
    r"| _ \(_) | | | | \| ||   \| __| _ \  | _ \ _ \ __/ __/ __| | | | _ \ __| ",
    r"|  _/| | | |_| | .` || |) | _||   /  |  _/   / _|\__ \__ \ |_| |   / _|  ",
    r"|_|  |_|  \___/|_|\_||___/|___|_|_\  |_| |_|_\___|___/___/\___/|_|_\___| ",
];

/// Height constants for layout
const TITLE_HEIGHT: u16 = 11; // ASCII (8) + timer line (1) + borders (2)
const MEM_HEIGHT: u16 = 8; // Memory section (6 lines + 2 border)
const PROGRESS_HEIGHT: u16 = 3; // Progress bar section (1 content + 2 border)
const FOOTER_HEIGHT: u16 = 7; // Footer (4 content + 2 border + 1 padding)

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
    _update_interval: u64,
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

    // Calculate CPU stats height based on number of cores (2 header lines + per-core bars + temp bar + borders)
    let num_cores = stats.cpu_usage_per_core.len().max(4);
    let cpu_height = (2 + num_cores + 1 + 2) as u16; // +1 for temp bar, +2 for borders

    // Create main layout with 5 sections (vertical stacking)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(TITLE_HEIGHT),    // ASCII art title
            Constraint::Length(cpu_height),      // CPU stats
            Constraint::Length(MEM_HEIGHT),      // Memory & Storage
            Constraint::Length(PROGRESS_HEIGHT), // Progress
            Constraint::Length(FOOTER_HEIGHT),   // Footer
        ])
        .split(size);

    // Title with ASCII art
    render_title(frame, chunks[0], stats);

    // CPU Stats
    render_cpu_stats(frame, chunks[1], stats);

    // Memory & Storage Stats
    render_memory_stats(frame, chunks[2], stats);

    // Progress
    render_progress(frame, chunks[3], stats, total_secs);

    // Footer
    render_footer(frame, chunks[4]);
}

fn render_title(frame: &mut Frame, area: Rect, stats: &StressStats) {
    let elapsed = format_duration(stats.elapsed_secs);

    // Build ASCII art lines: fire gradient on flames (top), solid color on text (bottom)
    let fire_colors = [
        Color::Red,         // Line 0 - flame tips
        Color::Red,         // Line 1
        Color::LightRed,    // Line 2
        Color::Yellow,      // Line 3
        Color::LightYellow, // Line 4 - hottest flames
    ];

    let mut lines: Vec<Line> = ASCII_TITLE
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // Lines 0-4: fire gradient, Lines 5-7: solid cyan for text
            let color = if i < 5 {
                fire_colors.get(i).copied().unwrap_or(Color::Yellow)
            } else {
                Color::Cyan // Solid color for "Pi UNDER PRESSURE" text
            };
            Line::from(Span::styled(
                *line,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();

    // Add timer line below ASCII art (with version)
    let version = env!("CARGO_PKG_VERSION");
    lines.push(Line::from(vec![
        Span::raw("                         "),
        Span::styled("─ Stability Tester ", Style::default().fg(Color::White)),
        Span::styled(
            format!("v{}", version),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" ─ ", Style::default().fg(Color::White)),
        Span::styled(
            elapsed,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let title = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(title, area);
}

fn render_cpu_stats(frame: &mut Frame, area: Rect, stats: &StressStats) {
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
        Span::styled(
            "YES - THROTTLING!",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("None", Style::default().fg(Color::Green))
    };

    // Fan status string
    let fan_str = match (stats.fan_status.speed_percent, stats.fan_status.rpm) {
        (Some(pct), Some(rpm)) => format!("{}% ({} RPM)", pct, rpm),
        (Some(pct), None) => format!("{}%", pct),
        (None, Some(rpm)) => format!("{} RPM", rpm),
        (None, None) => "N/A".to_string(),
    };

    // Build CPU info lines including per-core usage
    let mut cpu_lines = vec![
        Line::from(vec![
            Span::raw("  Temp: "),
            Span::styled(
                format!("{:.1}°C", stats.cpu_temp_c),
                Style::default()
                    .fg(cpu_temp_color)
                    .add_modifier(Modifier::BOLD),
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
                Style::default().fg(if stats.cpu_errors > 0 {
                    Color::Red
                } else {
                    Color::Green
                }),
            ),
            Span::raw("  Fan: "),
            Span::styled(fan_str, Style::default().fg(Color::Cyan)),
        ]),
    ];

    // Add per-core CPU usage bars (wider bars for vertical layout)
    for (i, usage) in stats.cpu_usage_per_core.iter().enumerate() {
        let bar_width = 40;
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
                "━".repeat(filled.min(bar_width)),
                Style::default().fg(bar_color),
            ),
            Span::styled(
                "─".repeat(bar_width.saturating_sub(filled)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("] "),
            Span::styled(format!("{:5.1}%", usage), Style::default().fg(bar_color)),
        ]));
    }

    // Add CPU temperature bar
    let temp_percent = ((stats.cpu_temp_c / 100.0) * 100.0) as u16;
    let temp_bar_color = if stats.cpu_temp_c >= 85.0 {
        Color::Red
    } else if stats.cpu_temp_c >= 80.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    let temp_bar_width = 40;
    let temp_filled = (temp_percent as usize * temp_bar_width / 100).min(temp_bar_width);
    cpu_lines.push(Line::from(vec![
        Span::raw("  Temp: ["),
        Span::styled("█".repeat(temp_filled), Style::default().fg(temp_bar_color)),
        Span::styled(
            "░".repeat(temp_bar_width.saturating_sub(temp_filled)),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("] "),
        Span::styled(
            format!("{:.1}°C / 85°C", stats.cpu_temp_c),
            Style::default().fg(temp_bar_color),
        ),
    ]));

    let cpu_info = Paragraph::new(cpu_lines).block(
        Block::default()
            .title(" CPU ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(cpu_info, area);
}

fn render_memory_stats(frame: &mut Frame, area: Rect, stats: &StressStats) {
    let nvme_temp_str = match stats.nvme_temp_c {
        Some(temp) => format!("{:.1}°C", temp),
        None => "N/A".to_string(),
    };

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
            Span::raw("  Memory Errors:    "),
            Span::styled(
                format!("{}", stats.memory_errors),
                Style::default().fg(if stats.memory_errors > 0 {
                    Color::Red
                } else {
                    Color::Green
                }),
            ),
        ]),
        Line::from(vec![
            Span::raw("  I/O Errors:       "),
            Span::styled(
                format!("{}", stats.io_errors),
                Style::default().fg(if stats.io_errors > 0 {
                    Color::Red
                } else {
                    Color::Green
                }),
            ),
        ]),
        Line::from(vec![
            Span::raw("  NVMe Temperature: "),
            Span::styled(nvme_temp_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("  NVMe Test File:   "),
            Span::styled(nvme_test_path_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("  Video Errors:     "),
            Span::styled(
                format!("{}", stats.video_errors),
                Style::default().fg(if stats.video_errors > 0 {
                    Color::Red
                } else {
                    Color::Green
                }),
            ),
        ]),
    ])
    .block(
        Block::default()
            .title(" Memory & Storage ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(mem_info, area);
}

fn render_progress(frame: &mut Frame, area: Rect, stats: &StressStats, total_secs: u64) {
    let remaining_secs = total_secs.saturating_sub(stats.elapsed_secs);
    let remaining_str = format_duration(remaining_secs);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .percent((stats.progress_percent as u16).min(100))
        .label(format!(
            "{:.1}% | Remaining: {}",
            stats.progress_percent, remaining_str
        ));

    frame.render_widget(gauge, area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  GitHub: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "https://github.com/cmd0s/Pi-Under-Pressure",
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Guide:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[Overclocking Guide - Coming Soon]",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
        Line::from(vec![Span::styled(
            "  Run with '-h' to see all options",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(vec![Span::styled(
            "  Press 'q' or Ctrl+C to stop test gracefully",
            Style::default().fg(Color::White),
        )]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(footer, area);
}
