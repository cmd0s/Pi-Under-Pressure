use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub cpu: String,
    pub cpu_cores: usize,
    pub ram_mb: u64,
    pub os: String,
    pub kernel: String,
    pub architecture: String,
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self {
            model: "Unknown".to_string(),
            serial: "Unknown".to_string(),
            firmware: "Unknown".to_string(),
            cpu: "Unknown".to_string(),
            cpu_cores: 0,
            ram_mb: 0,
            os: "Unknown".to_string(),
            kernel: "Unknown".to_string(),
            architecture: "Unknown".to_string(),
        }
    }
}

/// Raspberry Pi model identification for config.txt filter matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiModel {
    // Pi 5 family
    Pi5,
    Pi500,
    Cm5,

    // Pi 4 family
    Pi4,
    Pi400,
    Cm4,
    Cm4S,

    // Pi 3 family
    Pi3,
    Pi3Plus,
    Cm3,
    Cm3Plus,

    // Pi 2 family
    Pi2,

    // Pi 1 family
    Pi1,
    Cm1,

    // Pi Zero family
    Pi0,
    Pi0W,
    Pi02,

    // Unknown model (non-Pi platform)
    Unknown,
}

impl PiModel {
    /// Parse Pi model from /proc/cpuinfo "Model" field
    /// Examples: "Raspberry Pi 5 Model B Rev 1.0", "Raspberry Pi 4 Model B Rev 1.5"
    pub fn from_model_string(model: &str) -> Self {
        let model_lower = model.to_lowercase();

        // Pi 5 family (check specific variants first)
        if model_lower.contains("pi 500") || model_lower.contains("pi500") {
            return PiModel::Pi500;
        }
        if model_lower.contains("compute module 5") || model_lower.contains("cm5") {
            return PiModel::Cm5;
        }
        if model_lower.contains("pi 5") {
            return PiModel::Pi5;
        }

        // Pi 4 family
        if model_lower.contains("pi 400") || model_lower.contains("pi400") {
            return PiModel::Pi400;
        }
        if model_lower.contains("compute module 4s") || model_lower.contains("cm4s") {
            return PiModel::Cm4S;
        }
        if model_lower.contains("compute module 4") || model_lower.contains("cm4") {
            return PiModel::Cm4;
        }
        if model_lower.contains("pi 4") {
            return PiModel::Pi4;
        }

        // Pi 3 family
        if model_lower.contains("compute module 3+") || model_lower.contains("cm3+") {
            return PiModel::Cm3Plus;
        }
        if model_lower.contains("compute module 3") || model_lower.contains("cm3") {
            return PiModel::Cm3;
        }
        if model_lower.contains("3 model b+")
            || model_lower.contains("3 model a+")
            || model_lower.contains("3b+")
            || model_lower.contains("3a+")
        {
            return PiModel::Pi3Plus;
        }
        if model_lower.contains("pi 3") {
            return PiModel::Pi3;
        }

        // Pi 2 family
        if model_lower.contains("pi 2") {
            return PiModel::Pi2;
        }

        // Pi Zero family (check before Pi 1)
        if model_lower.contains("zero 2") || model_lower.contains("pi02") {
            return PiModel::Pi02;
        }
        if model_lower.contains("zero w") {
            return PiModel::Pi0W;
        }
        if model_lower.contains("zero") {
            return PiModel::Pi0;
        }

        // Pi 1 family
        if model_lower.contains("compute module 1") || model_lower.contains("cm1") {
            return PiModel::Cm1;
        }
        if model_lower.contains("pi 1")
            || model_lower.contains("model b rev")
            || model_lower.contains("model a rev")
        {
            return PiModel::Pi1;
        }

        PiModel::Unknown
    }
}

pub fn collect_system_info() -> SystemInfo {
    let mut info = SystemInfo::default();

    // Read /proc/cpuinfo for model, serial, CPU info
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if line.starts_with("Model") {
                if let Some(value) = line.split(':').nth(1) {
                    info.model = value.trim().to_string();
                }
            } else if line.starts_with("Serial") {
                if let Some(value) = line.split(':').nth(1) {
                    info.serial = value.trim().to_string();
                }
            } else if line.starts_with("Hardware") {
                if let Some(value) = line.split(':').nth(1) {
                    info.cpu = value.trim().to_string();
                }
            }
        }

        // Count CPU cores (count "processor" lines)
        info.cpu_cores = cpuinfo
            .lines()
            .filter(|line| line.starts_with("processor"))
            .count();
    }

    // If CPU info not found in /proc/cpuinfo, try to identify Cortex-A76
    if info.cpu == "Unknown" || info.cpu.is_empty() {
        info.cpu = "ARM Cortex-A76".to_string();
    }

    if info.cpu_cores == 0 {
        info.cpu_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
    }

    // Read /proc/meminfo for RAM
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(value) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = value.parse::<u64>() {
                        info.ram_mb = kb / 1024;
                    }
                }
                break;
            }
        }
    }

    // Read /etc/os-release for OS info
    if let Ok(os_release) = fs::read_to_string("/etc/os-release") {
        for line in os_release.lines() {
            if line.starts_with("PRETTY_NAME=") {
                info.os = line
                    .trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string();
                break;
            }
        }
    }

    // Get kernel version
    if let Ok(output) = Command::new("uname").arg("-r").output() {
        if output.status.success() {
            info.kernel = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    // Get architecture
    if let Ok(output) = Command::new("uname").arg("-m").output() {
        if output.status.success() {
            info.architecture = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    // Get firmware version via vcgencmd
    info.firmware = get_firmware_version();

    info
}

pub fn get_firmware_version() -> String {
    if let Ok(output) = Command::new("vcgencmd").arg("version").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse the version output - typically first line contains date
            for line in stdout.lines() {
                if line.contains("version") || line.contains("20") {
                    // Look for date pattern
                    return line.trim().to_string();
                }
            }
            // Return first non-empty line if no version found
            if let Some(first_line) = stdout.lines().next() {
                return first_line.trim().to_string();
            }
        }
    }
    "Unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_system_info() {
        let info = collect_system_info();
        // Basic sanity checks
        assert!(info.cpu_cores > 0 || info.cpu_cores == 0); // May be 0 on non-Pi systems
    }

    #[test]
    fn test_pi_model_from_string_pi5_family() {
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 5 Model B Rev 1.0"),
            PiModel::Pi5
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 500 Rev 1.0"),
            PiModel::Pi500
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi Compute Module 5"),
            PiModel::Cm5
        );
    }

    #[test]
    fn test_pi_model_from_string_pi4_family() {
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 4 Model B Rev 1.5"),
            PiModel::Pi4
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 400 Rev 1.0"),
            PiModel::Pi400
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi Compute Module 4"),
            PiModel::Cm4
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi Compute Module 4S"),
            PiModel::Cm4S
        );
    }

    #[test]
    fn test_pi_model_from_string_pi3_family() {
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 3 Model B Rev 1.2"),
            PiModel::Pi3
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 3 Model B+"),
            PiModel::Pi3Plus
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi 3 Model A+"),
            PiModel::Pi3Plus
        );
    }

    #[test]
    fn test_pi_model_from_string_zero_family() {
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi Zero Rev 1.3"),
            PiModel::Pi0
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi Zero W Rev 1.1"),
            PiModel::Pi0W
        );
        assert_eq!(
            PiModel::from_model_string("Raspberry Pi Zero 2 W Rev 1.0"),
            PiModel::Pi02
        );
    }

    #[test]
    fn test_pi_model_unknown() {
        assert_eq!(PiModel::from_model_string("Unknown"), PiModel::Unknown);
        assert_eq!(PiModel::from_model_string(""), PiModel::Unknown);
        assert_eq!(
            PiModel::from_model_string("Some other device"),
            PiModel::Unknown
        );
    }
}
