use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmeInfo {
    pub device_path: String,
    pub model: String,
    pub pcie_gen: Option<u32>,
}

/// Detect NVMe device
pub fn detect_nvme() -> Option<NvmeInfo> {
    // Check for NVMe devices in /sys/class/nvme
    let nvme_class = Path::new("/sys/class/nvme");
    if !nvme_class.exists() {
        return None;
    }

    // Look for nvme0
    let nvme0 = nvme_class.join("nvme0");
    if !nvme0.exists() {
        return None;
    }

    // Get model name
    let model = fs::read_to_string(nvme0.join("model"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Unknown NVMe".to_string());

    // Get device path
    let device_path = "/dev/nvme0n1".to_string();
    if !Path::new(&device_path).exists() {
        return None;
    }

    // Get PCIe generation
    let pcie_gen = get_pcie_generation();

    Some(NvmeInfo {
        device_path,
        model,
        pcie_gen,
    })
}

/// Get PCIe generation for NVMe device
pub fn get_pcie_generation() -> Option<u32> {
    // Try lspci first
    if let Ok(output) = Command::new("lspci").args(["-vvv"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut in_nvme_section = false;

            for line in stdout.lines() {
                // Look for NVMe controller
                if line.contains("NVMe") || line.contains("Non-Volatile memory controller") {
                    in_nvme_section = true;
                    continue;
                }

                // Look for LnkSta in NVMe section
                if in_nvme_section && line.contains("LnkSta:") {
                    // Format: LnkSta: Speed 8GT/s, Width x1
                    // Gen 1 = 2.5GT/s, Gen 2 = 5GT/s, Gen 3 = 8GT/s
                    if line.contains("8GT/s") {
                        return Some(3);
                    } else if line.contains("5GT/s") {
                        return Some(2);
                    } else if line.contains("2.5GT/s") {
                        return Some(1);
                    }
                }

                // Check for new device section
                if !line.starts_with('\t') && !line.starts_with(' ') && !line.is_empty() {
                    in_nvme_section = false;
                }
            }
        }
    }

    // Try reading from sysfs
    let link_speed_path = "/sys/class/nvme/nvme0/device/current_link_speed";
    if let Ok(speed) = fs::read_to_string(link_speed_path) {
        let speed = speed.trim();
        if speed.contains("8.0 GT/s") || speed.contains("8 GT/s") {
            return Some(3);
        } else if speed.contains("5.0 GT/s") || speed.contains("5 GT/s") {
            return Some(2);
        } else if speed.contains("2.5 GT/s") {
            return Some(1);
        }
    }

    None
}

/// Get NVMe temperature
pub fn get_nvme_temp(device_path: &str) -> Option<f32> {
    // Try hwmon first (most reliable)
    if let Some(temp) = get_nvme_temp_hwmon() {
        return Some(temp);
    }

    // Try smartctl
    if let Some(temp) = get_nvme_temp_smartctl(device_path) {
        return Some(temp);
    }

    // Try nvme-cli
    if let Some(temp) = get_nvme_temp_nvmecli(device_path) {
        return Some(temp);
    }

    None
}

/// Get NVMe temperature from hwmon
fn get_nvme_temp_hwmon() -> Option<f32> {
    let hwmon_path = Path::new("/sys/class/hwmon");
    if !hwmon_path.exists() {
        return None;
    }

    // Iterate through hwmon devices
    if let Ok(entries) = fs::read_dir(hwmon_path) {
        for entry in entries.flatten() {
            let path = entry.path();

            // Check if this is an NVMe hwmon device
            let name_path = path.join("name");
            if let Ok(name) = fs::read_to_string(&name_path) {
                if name.trim().contains("nvme") {
                    // Read temperature
                    let temp_path = path.join("temp1_input");
                    if let Ok(temp_str) = fs::read_to_string(&temp_path) {
                        if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                            return Some(temp_millic as f32 / 1000.0);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Get NVMe temperature using smartctl
fn get_nvme_temp_smartctl(device_path: &str) -> Option<f32> {
    if let Ok(output) = Command::new("smartctl").args(["-A", device_path]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("Temperature:") || line.contains("Temperature Sensor") {
                    // Try to extract temperature value
                    for word in line.split_whitespace() {
                        if let Ok(temp) = word.parse::<f32>() {
                            if temp > 0.0 && temp < 150.0 {
                                // Sanity check
                                return Some(temp);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Get NVMe temperature using nvme-cli
fn get_nvme_temp_nvmecli(device_path: &str) -> Option<f32> {
    // Strip partition number if present
    let device = device_path.trim_end_matches(|c: char| c.is_numeric());
    let device = if device.ends_with('p') {
        device.trim_end_matches('p')
    } else {
        device
    };

    if let Ok(output) = Command::new("nvme").args(["smart-log", device]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.to_lowercase().contains("temperature") {
                    // Format varies, try to extract number
                    for word in line.split_whitespace() {
                        // Remove common suffixes
                        let cleaned = word.trim_end_matches('C').trim_end_matches('°');
                        if let Ok(temp) = cleaned.parse::<f32>() {
                            if temp > 0.0 && temp < 150.0 {
                                return Some(temp);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Get NVMe SMART health status
pub fn get_nvme_smart_status(device_path: &str) -> Option<SmartStatus> {
    let device = device_path.trim_end_matches(|c: char| c.is_numeric());
    let device = if device.ends_with('p') {
        device.trim_end_matches('p')
    } else {
        device
    };

    if let Ok(output) = Command::new("nvme").args(["smart-log", device]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut status = SmartStatus::default();

            for line in stdout.lines() {
                let line_lower = line.to_lowercase();

                if line_lower.contains("critical_warning") {
                    // Parse hex or decimal value
                    if let Some(value) = parse_smart_value(line) {
                        status.critical_warning = value as u8;
                    }
                } else if line_lower.contains("media_errors") || line_lower.contains("media errors")
                {
                    if let Some(value) = parse_smart_value(line) {
                        status.media_errors = value;
                    }
                } else if line_lower.contains("num_err_log")
                    || line_lower.contains("error log entries")
                {
                    if let Some(value) = parse_smart_value(line) {
                        status.error_log_entries = value;
                    }
                }
            }

            return Some(status);
        }
    }

    None
}

fn parse_smart_value(line: &str) -> Option<u64> {
    // Try to find a number after colon
    if let Some(colon_pos) = line.find(':') {
        let value_part = &line[colon_pos + 1..];
        for word in value_part.split_whitespace() {
            // Handle hex values
            if word.starts_with("0x") {
                if let Ok(v) = u64::from_str_radix(&word[2..], 16) {
                    return Some(v);
                }
            }
            // Handle decimal
            if let Ok(v) = word.replace(',', "").parse::<u64>() {
                return Some(v);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmartStatus {
    pub critical_warning: u8,
    pub media_errors: u64,
    pub error_log_entries: u64,
}

impl SmartStatus {
    pub fn has_issues(&self) -> bool {
        self.critical_warning != 0 || self.media_errors > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_nvme() {
        // Just ensure it doesn't panic
        let _ = detect_nvme();
    }

    #[test]
    fn test_get_pcie_generation() {
        let _ = get_pcie_generation();
    }

    #[test]
    fn test_parse_smart_value() {
        assert_eq!(parse_smart_value("critical_warning: 0x00"), Some(0));
        assert_eq!(parse_smart_value("media_errors: 0"), Some(0));
        assert_eq!(parse_smart_value("error entries: 1,234"), Some(1234));
    }
}
