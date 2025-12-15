use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ThrottleStatus {
    /// Under-voltage detected NOW
    pub under_voltage_now: bool,
    /// ARM frequency capped NOW
    pub freq_capped_now: bool,
    /// Currently throttled NOW
    pub throttled_now: bool,
    /// Soft temp limit active NOW
    pub soft_temp_limit_now: bool,
    /// Under-voltage occurred since boot
    pub under_voltage_occurred: bool,
    /// ARM frequency capped occurred since boot
    pub freq_capped_occurred: bool,
    /// Throttling occurred since boot
    pub throttled_occurred: bool,
    /// Soft temp limit occurred since boot
    pub soft_temp_limit_occurred: bool,
    /// Raw throttle value
    pub raw_value: u32,
}

impl ThrottleStatus {
    pub fn from_raw(raw: u32) -> Self {
        Self {
            under_voltage_now: (raw & (1 << 0)) != 0,
            freq_capped_now: (raw & (1 << 1)) != 0,
            throttled_now: (raw & (1 << 2)) != 0,
            soft_temp_limit_now: (raw & (1 << 3)) != 0,
            under_voltage_occurred: (raw & (1 << 16)) != 0,
            freq_capped_occurred: (raw & (1 << 17)) != 0,
            throttled_occurred: (raw & (1 << 18)) != 0,
            soft_temp_limit_occurred: (raw & (1 << 19)) != 0,
            raw_value: raw,
        }
    }

    pub fn has_any_current_issue(&self) -> bool {
        self.under_voltage_now
            || self.freq_capped_now
            || self.throttled_now
            || self.soft_temp_limit_now
    }

    pub fn has_any_historical_issue(&self) -> bool {
        self.under_voltage_occurred
            || self.freq_capped_occurred
            || self.throttled_occurred
            || self.soft_temp_limit_occurred
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitorStats {
    pub cpu_temp_c: f32,
    pub cpu_freq_mhz: u32,
    pub gpu_freq_mhz: u32,
    pub throttle_status: ThrottleStatus,
    pub governor: String,
    pub cpu_usage_percent: f32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
}

/// Get CPU temperature using vcgencmd
pub fn get_cpu_temp() -> f32 {
    if let Ok(output) = Command::new("vcgencmd").arg("measure_temp").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Output format: temp=45.0'C
            if let Some(temp_str) = stdout.strip_prefix("temp=") {
                let temp_str = temp_str.trim().trim_end_matches("'C");
                if let Ok(temp) = temp_str.parse::<f32>() {
                    return temp;
                }
            }
        }
    }

    // Fallback: try thermal zone
    if let Ok(temp_str) = fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
        if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
            return temp_millic as f32 / 1000.0;
        }
    }

    0.0
}

/// Get current CPU frequency using vcgencmd
pub fn get_cpu_freq() -> u32 {
    if let Ok(output) = Command::new("vcgencmd")
        .args(["measure_clock", "arm"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Output format: frequency(48)=2400000000
            if let Some(freq_part) = stdout.split('=').nth(1) {
                if let Ok(freq_hz) = freq_part.trim().parse::<u64>() {
                    return (freq_hz / 1_000_000) as u32;
                }
            }
        }
    }

    // Fallback: try sysfs
    if let Ok(freq_str) = fs::read_to_string(
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
    ) {
        if let Ok(freq_khz) = freq_str.trim().parse::<u32>() {
            return freq_khz / 1000;
        }
    }

    0
}

/// Get current GPU frequency using vcgencmd
pub fn get_gpu_freq() -> u32 {
    if let Ok(output) = Command::new("vcgencmd")
        .args(["measure_clock", "core"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Output format: frequency(1)=910000000
            if let Some(freq_part) = stdout.split('=').nth(1) {
                if let Ok(freq_hz) = freq_part.trim().parse::<u64>() {
                    return (freq_hz / 1_000_000) as u32;
                }
            }
        }
    }
    0
}

/// Get throttle status using vcgencmd
pub fn get_throttle_status() -> ThrottleStatus {
    if let Ok(output) = Command::new("vcgencmd").arg("get_throttled").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Output format: throttled=0x0
            if let Some(hex_str) = stdout.split("0x").nth(1) {
                if let Ok(raw) = u32::from_str_radix(hex_str.trim(), 16) {
                    return ThrottleStatus::from_raw(raw);
                }
            }
        }
    }
    ThrottleStatus::default()
}

/// Get CPU governor
pub fn get_governor() -> String {
    fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Get memory usage
pub fn get_memory_usage() -> (u64, u64) {
    let mut total_mb = 0u64;
    let mut available_mb = 0u64;

    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        total_mb = kb / 1024;
                    }
                }
            } else if line.starts_with("MemAvailable:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        available_mb = kb / 1024;
                    }
                }
            }
        }
    }

    let used_mb = total_mb.saturating_sub(available_mb);
    (used_mb, total_mb)
}

/// Collect all monitoring stats
pub fn collect_stats() -> MonitorStats {
    let (mem_used, mem_total) = get_memory_usage();

    MonitorStats {
        cpu_temp_c: get_cpu_temp(),
        cpu_freq_mhz: get_cpu_freq(),
        gpu_freq_mhz: get_gpu_freq(),
        throttle_status: get_throttle_status(),
        governor: get_governor(),
        cpu_usage_percent: 0.0, // Will be calculated by stress module
        mem_used_mb: mem_used,
        mem_total_mb: mem_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttle_status_from_raw() {
        // Test with no throttling
        let status = ThrottleStatus::from_raw(0x0);
        assert!(!status.has_any_current_issue());
        assert!(!status.has_any_historical_issue());

        // Test with under-voltage occurred
        let status = ThrottleStatus::from_raw(0x50000);
        assert!(!status.has_any_current_issue());
        assert!(status.has_any_historical_issue());
        assert!(status.under_voltage_occurred);

        // Test with current throttling
        let status = ThrottleStatus::from_raw(0x4);
        assert!(status.has_any_current_issue());
        assert!(status.throttled_now);
    }
}
