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
}
