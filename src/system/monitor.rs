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
pub struct FanStatus {
    /// Fan speed as percentage (0-100)
    pub speed_percent: Option<u8>,
    /// Fan speed in RPM (if available)
    pub rpm: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitorStats {
    pub cpu_temp_c: f32,
    pub cpu_freq_mhz: u32,
    pub gpu_freq_mhz: u32,
    pub throttle_status: ThrottleStatus,
    pub governor: String,
    pub cpu_usage_percent: f32,
    pub cpu_usage_per_core: Vec<f32>,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub fan_status: FanStatus,
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

/// Get minimum CPU frequency from sysfs (in MHz)
pub fn get_cpu_freq_min() -> u32 {
    if let Ok(freq_str) = fs::read_to_string(
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq",
    ) {
        if let Ok(freq_khz) = freq_str.trim().parse::<u32>() {
            return freq_khz / 1000;
        }
    }
    0
}

/// Get maximum CPU frequency from sysfs (in MHz)
pub fn get_cpu_freq_max() -> u32 {
    if let Ok(freq_str) = fs::read_to_string(
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq",
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

/// CPU stat snapshot for calculating usage
#[derive(Default, Clone)]
pub struct CpuStatSnapshot {
    /// Per-core stats: (user, nice, system, idle, iowait, irq, softirq)
    pub cores: Vec<(u64, u64, u64, u64, u64, u64, u64)>,
}

impl CpuStatSnapshot {
    /// Read current CPU stats from /proc/stat
    pub fn read() -> Self {
        let mut cores = Vec::new();

        if let Ok(stat) = fs::read_to_string("/proc/stat") {
            for line in stat.lines() {
                // Look for lines like "cpu0", "cpu1", etc.
                if line.starts_with("cpu") && !line.starts_with("cpu ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 8 {
                        let user = parts[1].parse().unwrap_or(0);
                        let nice = parts[2].parse().unwrap_or(0);
                        let system = parts[3].parse().unwrap_or(0);
                        let idle = parts[4].parse().unwrap_or(0);
                        let iowait = parts[5].parse().unwrap_or(0);
                        let irq = parts[6].parse().unwrap_or(0);
                        let softirq = parts[7].parse().unwrap_or(0);
                        cores.push((user, nice, system, idle, iowait, irq, softirq));
                    }
                }
            }
        }

        Self { cores }
    }

    /// Calculate per-core usage percentage compared to a previous snapshot
    pub fn calculate_usage(&self, prev: &CpuStatSnapshot) -> Vec<f32> {
        let mut usage = Vec::new();

        for (i, curr) in self.cores.iter().enumerate() {
            if let Some(prev_core) = prev.cores.get(i) {
                let curr_total = curr.0 + curr.1 + curr.2 + curr.3 + curr.4 + curr.5 + curr.6;
                let prev_total =
                    prev_core.0 + prev_core.1 + prev_core.2 + prev_core.3 + prev_core.4 + prev_core.5 + prev_core.6;

                let curr_idle = curr.3 + curr.4;
                let prev_idle = prev_core.3 + prev_core.4;

                let total_diff = curr_total.saturating_sub(prev_total);
                let idle_diff = curr_idle.saturating_sub(prev_idle);

                if total_diff > 0 {
                    let usage_pct = ((total_diff - idle_diff) as f32 / total_diff as f32) * 100.0;
                    usage.push(usage_pct.clamp(0.0, 100.0));
                } else {
                    usage.push(0.0);
                }
            } else {
                usage.push(0.0);
            }
        }

        usage
    }
}

/// Get fan speed from hwmon or vcgencmd
pub fn get_fan_status() -> FanStatus {
    let mut status = FanStatus::default();

    // Try to find PWM fan in hwmon
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();

            // Check for PWM value (0-255)
            let pwm_path = path.join("pwm1");
            if pwm_path.exists() {
                if let Ok(pwm_str) = fs::read_to_string(&pwm_path) {
                    if let Ok(pwm) = pwm_str.trim().parse::<u32>() {
                        // Convert 0-255 to percentage
                        status.speed_percent = Some(((pwm * 100) / 255).min(100) as u8);
                    }
                }
            }

            // Check for fan RPM
            let rpm_path = path.join("fan1_input");
            if rpm_path.exists() {
                if let Ok(rpm_str) = fs::read_to_string(&rpm_path) {
                    if let Ok(rpm) = rpm_str.trim().parse::<u32>() {
                        status.rpm = Some(rpm);
                    }
                }
            }

            // If we found fan data, stop looking
            if status.speed_percent.is_some() || status.rpm.is_some() {
                break;
            }
        }
    }

    // Fallback: try Raspberry Pi 5 specific fan control
    if status.speed_percent.is_none() {
        // RPi5 official cooler uses cooling_device interface
        if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.to_string_lossy().contains("cooling_device") {
                    let cur_state = path.join("cur_state");
                    let max_state = path.join("max_state");

                    if let (Ok(cur_str), Ok(max_str)) = (
                        fs::read_to_string(&cur_state),
                        fs::read_to_string(&max_state),
                    ) {
                        if let (Ok(cur), Ok(max)) = (
                            cur_str.trim().parse::<u32>(),
                            max_str.trim().parse::<u32>(),
                        ) {
                            if max > 0 {
                                status.speed_percent = Some(((cur * 100) / max).min(100) as u8);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    status
}

/// Collect all monitoring stats (without per-core CPU usage)
pub fn collect_stats() -> MonitorStats {
    let (mem_used, mem_total) = get_memory_usage();

    MonitorStats {
        cpu_temp_c: get_cpu_temp(),
        cpu_freq_mhz: get_cpu_freq(),
        gpu_freq_mhz: get_gpu_freq(),
        throttle_status: get_throttle_status(),
        governor: get_governor(),
        cpu_usage_percent: 0.0, // Will be calculated by stress module
        cpu_usage_per_core: Vec::new(), // Will be calculated with CpuStatSnapshot
        mem_used_mb: mem_used,
        mem_total_mb: mem_total,
        fan_status: get_fan_status(),
    }
}

/// Collect stats with per-core CPU usage calculation
pub fn collect_stats_with_cpu(prev_snapshot: &CpuStatSnapshot) -> (MonitorStats, CpuStatSnapshot) {
    let (mem_used, mem_total) = get_memory_usage();
    let current_snapshot = CpuStatSnapshot::read();
    let per_core = current_snapshot.calculate_usage(prev_snapshot);

    let avg_usage = if per_core.is_empty() {
        0.0
    } else {
        per_core.iter().sum::<f32>() / per_core.len() as f32
    };

    let stats = MonitorStats {
        cpu_temp_c: get_cpu_temp(),
        cpu_freq_mhz: get_cpu_freq(),
        gpu_freq_mhz: get_gpu_freq(),
        throttle_status: get_throttle_status(),
        governor: get_governor(),
        cpu_usage_percent: avg_usage,
        cpu_usage_per_core: per_core,
        mem_used_mb: mem_used,
        mem_total_mb: mem_total,
        fan_status: get_fan_status(),
    };

    (stats, current_snapshot)
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
