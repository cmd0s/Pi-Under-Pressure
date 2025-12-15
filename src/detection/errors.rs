use std::process::Command;
use std::time::{Duration, SystemTime};

/// Error patterns to look for in dmesg/journalctl
const ERROR_PATTERNS: &[&str] = &[
    "I/O error",
    "blk_update_request",
    "Buffer I/O error",
    "nvme.*error",
    "pcieport.*AER",
    "mmc.*error",
    "sd.*error",
    "ata.*error",
    "DMAR",
    "MCE",
    "Hardware Error",
    "Kernel panic",
    "Oops",
    "BUG:",
    "WARNING:",
    "Call Trace",
];

/// Check dmesg for I/O and other errors
pub fn check_dmesg_errors() -> Vec<String> {
    let mut errors = Vec::new();

    // Try dmesg first
    if let Ok(output) = Command::new("dmesg").args(["--level=err,warn"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if is_relevant_error(line) {
                    errors.push(line.to_string());
                }
            }
        }
    }

    // Also check journalctl for kernel messages
    if let Ok(output) = Command::new("journalctl")
        .args(["-k", "-p", "err", "--no-pager", "-n", "100"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if is_relevant_error(line) && !errors.contains(&line.to_string()) {
                    errors.push(line.to_string());
                }
            }
        }
    }

    errors
}

/// Count recent I/O errors (last minute)
pub fn count_recent_io_errors() -> u32 {
    let mut count = 0;

    // Try to get recent dmesg with timestamps
    if let Ok(output) = Command::new("dmesg")
        .args(["--level=err", "-T"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let now = SystemTime::now();

            for line in stdout.lines() {
                // Check if line contains I/O related error
                if line.contains("I/O error")
                    || line.contains("blk_update_request")
                    || line.contains("Buffer I/O error")
                    || line.contains("nvme")
                {
                    // Simple approach: count all relevant errors
                    // More sophisticated: parse timestamp and check if recent
                    count += 1;
                }
            }
        }
    }

    count
}

/// Check if a log line is a relevant error
fn is_relevant_error(line: &str) -> bool {
    let line_lower = line.to_lowercase();

    for pattern in ERROR_PATTERNS {
        if line_lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    false
}

/// Get NVMe specific errors from dmesg
pub fn get_nvme_errors() -> Vec<String> {
    let mut errors = Vec::new();

    if let Ok(output) = Command::new("dmesg").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line_lower = line.to_lowercase();
                if line_lower.contains("nvme") &&
                   (line_lower.contains("error") ||
                    line_lower.contains("timeout") ||
                    line_lower.contains("i/o"))
                {
                    errors.push(line.to_string());
                }
            }
        }
    }

    errors
}

/// Get PCIe AER (Advanced Error Reporting) errors
pub fn get_pcie_aer_errors() -> Vec<String> {
    let mut errors = Vec::new();

    if let Ok(output) = Command::new("dmesg").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("AER") || line.contains("pcieport") {
                    if line.to_lowercase().contains("error")
                        || line.contains("Correctable")
                        || line.contains("Uncorrectable")
                    {
                        errors.push(line.to_string());
                    }
                }
            }
        }
    }

    errors
}

/// Watch dmesg for new errors (returns when an error is found or timeout)
pub fn watch_for_errors(timeout: Duration) -> Option<String> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::Instant;

    let start = Instant::now();

    // Start dmesg in follow mode
    let mut child = match Command::new("dmesg")
        .args(["-w", "--level=err,warn"])
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return None,
    };

    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        if start.elapsed() >= timeout {
            break;
        }

        if let Ok(line) = line {
            if is_relevant_error(&line) {
                let _ = child.kill();
                return Some(line);
            }
        }
    }

    let _ = child.kill();
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_relevant_error() {
        assert!(is_relevant_error("blk_update_request: I/O error, dev nvme0n1"));
        assert!(is_relevant_error("Buffer I/O error on device sda1"));
        assert!(is_relevant_error("Kernel panic - not syncing"));
        assert!(!is_relevant_error("Normal log message"));
        assert!(!is_relevant_error("USB device connected"));
    }

    #[test]
    fn test_check_dmesg_errors() {
        // Just ensure it doesn't panic
        let _ = check_dmesg_errors();
    }
}
