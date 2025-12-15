pub mod simple;
pub mod tui;

use crate::detection::nvme::NvmeInfo;
use crate::stress::FinalReport;
use crate::system::config::OcConfig;
use crate::system::info::SystemInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Tui,
    Simple,
}

/// Display system information at startup
pub fn display_system_info(
    sys_info: &SystemInfo,
    oc_config: &OcConfig,
    nvme_info: &Option<NvmeInfo>,
    mode: UiMode,
    no_color: bool,
) {
    match mode {
        UiMode::Tui => {
            // For TUI mode, we'll show this briefly before starting the TUI
            simple::display_system_info(sys_info, oc_config, nvme_info, no_color);
        }
        UiMode::Simple => {
            simple::display_system_info(sys_info, oc_config, nvme_info, no_color);
        }
    }
}

/// Display final report
pub fn display_final_report(report: &FinalReport, no_color: bool) {
    simple::display_final_report(report, no_color);
}

/// Format duration as HH:MM:SS
pub fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

/// Format temperature with color indicator
pub fn format_temp(temp: f32, no_color: bool) -> String {
    let temp_str = format!("{:.1}°C", temp);

    if no_color {
        return temp_str;
    }

    // Color based on temperature range
    if temp >= 85.0 {
        format!("\x1b[31m{}\x1b[0m", temp_str) // Red - critical
    } else if temp >= 80.0 {
        format!("\x1b[33m{}\x1b[0m", temp_str) // Yellow - warning
    } else if temp >= 70.0 {
        format!("\x1b[93m{}\x1b[0m", temp_str) // Light yellow - warm
    } else {
        format!("\x1b[32m{}\x1b[0m", temp_str) // Green - OK
    }
}
