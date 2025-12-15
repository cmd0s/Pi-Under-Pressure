use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

const CONFIG_PATH: &str = "/boot/firmware/config.txt";
const CONFIG_PATH_ALT: &str = "/boot/config.txt";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OcConfig {
    /// CPU frequency in MHz (default: 2400)
    pub arm_freq: Option<u32>,
    /// GPU/Core frequency in MHz (default: 910)
    pub gpu_freq: Option<u32>,
    /// Core frequency in MHz (alias for gpu_freq on Pi5)
    pub core_freq: Option<u32>,
    /// Legacy voltage setting (0-8)
    pub over_voltage: Option<i32>,
    /// Voltage delta in µV (preferred for Pi5)
    pub over_voltage_delta: Option<i32>,
    /// Fixed core frequency (1 = enabled)
    pub core_freq_fixed: Option<u32>,
    /// Force max frequency (1 = enabled)
    pub force_turbo: Option<u32>,
    /// PCIe x1 enabled
    pub pcie_enabled: bool,
    /// PCIe generation (2 or 3)
    pub pcie_gen: Option<u32>,
    /// All raw config values
    pub raw_values: HashMap<String, String>,
}

impl OcConfig {
    pub fn is_overclocked(&self) -> bool {
        self.arm_freq.map(|f| f > 2400).unwrap_or(false)
            || self.gpu_freq.map(|f| f > 910).unwrap_or(false)
            || self.over_voltage.is_some()
            || self.over_voltage_delta.is_some()
            || self.force_turbo == Some(1)
    }

    pub fn voltage_offset_mv(&self) -> Option<f32> {
        if let Some(delta) = self.over_voltage_delta {
            // over_voltage_delta is in µV
            return Some(delta as f32 / 1000.0);
        }
        if let Some(ov) = self.over_voltage {
            // over_voltage steps: each step is roughly 25mV
            // 0 = 0.88V, 8 = 1.00V (approx 15mV per step)
            return Some(ov as f32 * 25.0);
        }
        None
    }
}

pub fn parse_config() -> OcConfig {
    let mut config = OcConfig::default();

    // Try primary path first, then alternative
    let content = fs::read_to_string(CONFIG_PATH)
        .or_else(|_| fs::read_to_string(CONFIG_PATH_ALT))
        .unwrap_or_default();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments, empty lines, and section headers like [all], [pi5], etc.
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }

        // Handle dtparam separately
        if line.starts_with("dtparam=") {
            let param = line.trim_start_matches("dtparam=");
            if param == "pciex1" || param.starts_with("pciex1=") {
                config.pcie_enabled = true;
            } else if param.starts_with("pciex1_gen=") {
                if let Some(gen_str) = param.strip_prefix("pciex1_gen=") {
                    if let Ok(gen) = gen_str.parse::<u32>() {
                        config.pcie_gen = Some(gen);
                    }
                }
            }
            continue;
        }

        // Parse key=value pairs
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Store raw value
            config.raw_values.insert(key.to_string(), value.to_string());

            match key {
                "arm_freq" => {
                    if let Ok(freq) = value.parse::<u32>() {
                        config.arm_freq = Some(freq);
                    }
                }
                "gpu_freq" => {
                    if let Ok(freq) = value.parse::<u32>() {
                        config.gpu_freq = Some(freq);
                    }
                }
                "core_freq" => {
                    if let Ok(freq) = value.parse::<u32>() {
                        config.core_freq = Some(freq);
                    }
                }
                "over_voltage" => {
                    if let Ok(ov) = value.parse::<i32>() {
                        config.over_voltage = Some(ov);
                    }
                }
                "over_voltage_delta" => {
                    if let Ok(delta) = value.parse::<i32>() {
                        config.over_voltage_delta = Some(delta);
                    }
                }
                "core_freq_fixed" => {
                    if let Ok(v) = value.parse::<u32>() {
                        config.core_freq_fixed = Some(v);
                    }
                }
                "force_turbo" => {
                    if let Ok(v) = value.parse::<u32>() {
                        config.force_turbo = Some(v);
                    }
                }
                _ => {}
            }
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_empty() {
        let config = OcConfig::default();
        assert!(!config.is_overclocked());
    }

    #[test]
    fn test_voltage_offset() {
        let mut config = OcConfig::default();

        // Test over_voltage_delta (in µV)
        config.over_voltage_delta = Some(50000);
        assert_eq!(config.voltage_offset_mv(), Some(50.0));

        // Test over_voltage (legacy)
        config.over_voltage_delta = None;
        config.over_voltage = Some(4);
        assert_eq!(config.voltage_offset_mv(), Some(100.0));
    }
}
