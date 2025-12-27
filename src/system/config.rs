use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::info::{collect_system_info, PiModel};

const CONFIG_PATH: &str = "/boot/firmware/config.txt";
const CONFIG_PATH_ALT: &str = "/boot/config.txt";

/// Config.txt section filter types
/// Based on Raspberry Pi documentation: https://www.raspberrypi.com/documentation/computers/config_txt.html
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFilter {
    /// [all] - applies to all devices
    All,
    /// [none] - applies to no devices
    None,

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

    // Pi 2
    Pi2,

    // Pi 1 family
    Pi1,
    Cm1,

    // Pi Zero family
    Pi0,
    Pi0W,
    Pi02,
}

impl ConfigFilter {
    /// Parse a filter from a config.txt section header line
    /// Returns None if the line is not a recognized filter
    pub fn from_line(line: &str) -> Option<Self> {
        let line = line.trim();
        if !line.starts_with('[') {
            return None;
        }

        // Find the closing bracket, handling comments like "[pi5] # comment"
        let end = line.find(']')?;
        let filter_name = &line[1..end].to_lowercase();

        match filter_name.as_str() {
            "all" => Some(ConfigFilter::All),
            "none" => Some(ConfigFilter::None),
            "pi5" => Some(ConfigFilter::Pi5),
            "pi500" => Some(ConfigFilter::Pi500),
            "cm5" => Some(ConfigFilter::Cm5),
            "pi4" => Some(ConfigFilter::Pi4),
            "pi400" => Some(ConfigFilter::Pi400),
            "cm4" => Some(ConfigFilter::Cm4),
            "cm4s" => Some(ConfigFilter::Cm4S),
            "pi3" => Some(ConfigFilter::Pi3),
            "pi3+" => Some(ConfigFilter::Pi3Plus),
            "cm3" => Some(ConfigFilter::Cm3),
            "cm3+" => Some(ConfigFilter::Cm3Plus),
            "pi2" => Some(ConfigFilter::Pi2),
            "pi1" => Some(ConfigFilter::Pi1),
            "cm1" => Some(ConfigFilter::Cm1),
            "pi0" => Some(ConfigFilter::Pi0),
            "pi0w" => Some(ConfigFilter::Pi0W),
            "pi02" => Some(ConfigFilter::Pi02),
            _ => None, // Unknown filter (e.g., [EDID=...], [tryboot])
        }
    }

    /// Check if this filter matches the given Pi model
    /// Implements the inheritance rules from Raspberry Pi documentation
    pub fn matches(&self, model: PiModel) -> bool {
        use ConfigFilter::*;
        use PiModel as M;

        match self {
            All => true,
            None => false,

            // Pi 5 family: [pi5] matches Pi5, Pi500, and CM5
            Pi5 => matches!(model, M::Pi5 | M::Pi500 | M::Cm5),
            Pi500 => model == M::Pi500,
            Cm5 => model == M::Cm5,

            // Pi 4 family: [pi4] matches Pi4, Pi400, CM4, and CM4S
            Pi4 => matches!(model, M::Pi4 | M::Pi400 | M::Cm4 | M::Cm4S),
            Pi400 => model == M::Pi400,
            Cm4 => model == M::Cm4,
            Cm4S => model == M::Cm4S,

            // Pi 3 family
            Pi3 => matches!(model, M::Pi3 | M::Cm3),
            Pi3Plus => matches!(model, M::Pi3Plus | M::Cm3Plus),
            Cm3 => model == M::Cm3,
            Cm3Plus => model == M::Cm3Plus,

            // Pi 2
            Pi2 => model == M::Pi2,

            // Pi 1 family: [pi1] matches Pi1 and CM1
            Pi1 => matches!(model, M::Pi1 | M::Cm1),
            Cm1 => model == M::Cm1,

            // Pi Zero family: [pi0] matches all zeros
            Pi0 => matches!(model, M::Pi0 | M::Pi0W | M::Pi02),
            Pi0W => model == M::Pi0W,
            Pi02 => model == M::Pi02,
        }
    }
}

/// Check if a model should apply settings from a given filter
/// This handles inheritance (e.g., Pi500 reads both [pi500] and [pi5])
fn should_apply_filter(filter: ConfigFilter, model: PiModel) -> bool {
    use ConfigFilter::*;
    use PiModel as M;

    // Direct match
    if filter.matches(model) {
        return true;
    }

    // Inheritance: specific models also read their parent filters
    match model {
        // Pi 5 family inheritance
        M::Pi500 | M::Cm5 => filter == Pi5,

        // Pi 4 family inheritance
        M::Pi400 | M::Cm4 | M::Cm4S => filter == Pi4,

        // Pi 3 family inheritance
        M::Cm3 => filter == Pi3,
        M::Cm3Plus => filter == Pi3Plus,

        // Pi 1 family inheritance
        M::Cm1 => filter == Pi1,

        // Pi Zero family inheritance
        M::Pi0W | M::Pi02 => filter == Pi0,

        _ => false,
    }
}

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

/// Parse config.txt with auto-detected Pi model
/// On non-Pi platforms, returns empty OcConfig
pub fn parse_config() -> OcConfig {
    let sys_info = collect_system_info();
    let model = PiModel::from_model_string(&sys_info.model);

    // On non-Pi platforms, return empty config
    if model == PiModel::Unknown {
        return OcConfig::default();
    }

    parse_config_for_model(model)
}

/// Parse config.txt for a specific Pi model
/// Use this for testing or when the model is already known
pub fn parse_config_for_model(model: PiModel) -> OcConfig {
    let content = fs::read_to_string(CONFIG_PATH)
        .or_else(|_| fs::read_to_string(CONFIG_PATH_ALT))
        .unwrap_or_default();

    parse_config_from_str(&content, model)
}

/// Parse config from a string (useful for testing)
pub fn parse_config_from_str(content: &str, model: PiModel) -> OcConfig {
    let mut config = OcConfig::default();

    // Track current filter state
    // None = beginning of file, applies to all (same as [all])
    let mut current_filter: Option<ConfigFilter> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Check for section filter headers
        if line.starts_with('[') {
            if let Some(filter) = ConfigFilter::from_line(line) {
                current_filter = Some(filter);
            }
            // Unknown filters keep previous state (ignore the line)
            continue;
        }

        // Check if current filter applies to this model
        let should_apply = match current_filter {
            Option::None => true, // Before any filter = applies to all
            Some(ConfigFilter::All) => true,
            Some(ConfigFilter::None) => false,
            Some(filter) => should_apply_filter(filter, model),
        };

        if !should_apply {
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

    #[test]
    fn test_config_filter_from_line() {
        assert_eq!(ConfigFilter::from_line("[all]"), Some(ConfigFilter::All));
        assert_eq!(ConfigFilter::from_line("[none]"), Some(ConfigFilter::None));
        assert_eq!(ConfigFilter::from_line("[pi5]"), Some(ConfigFilter::Pi5));
        assert_eq!(ConfigFilter::from_line("[pi4]"), Some(ConfigFilter::Pi4));
        assert_eq!(
            ConfigFilter::from_line("[pi3+]"),
            Some(ConfigFilter::Pi3Plus)
        );
        assert_eq!(ConfigFilter::from_line("[cm4s]"), Some(ConfigFilter::Cm4S));

        // Case insensitivity
        assert_eq!(ConfigFilter::from_line("[PI5]"), Some(ConfigFilter::Pi5));
        assert_eq!(ConfigFilter::from_line("[All]"), Some(ConfigFilter::All));

        // Comments after filter
        assert_eq!(
            ConfigFilter::from_line("[pi5] # comment"),
            Some(ConfigFilter::Pi5)
        );

        // Unknown filters
        assert_eq!(ConfigFilter::from_line("[EDID=something]"), None);
        assert_eq!(ConfigFilter::from_line("[tryboot]"), None);

        // Not a filter
        assert_eq!(ConfigFilter::from_line("arm_freq=2800"), None);
    }

    #[test]
    fn test_config_filter_matches() {
        // Pi 5 family
        assert!(ConfigFilter::Pi5.matches(PiModel::Pi5));
        assert!(ConfigFilter::Pi5.matches(PiModel::Pi500));
        assert!(ConfigFilter::Pi5.matches(PiModel::Cm5));
        assert!(!ConfigFilter::Pi5.matches(PiModel::Pi4));

        // Pi 4 family
        assert!(ConfigFilter::Pi4.matches(PiModel::Pi4));
        assert!(ConfigFilter::Pi4.matches(PiModel::Pi400));
        assert!(ConfigFilter::Pi4.matches(PiModel::Cm4));
        assert!(ConfigFilter::Pi4.matches(PiModel::Cm4S));
        assert!(!ConfigFilter::Pi4.matches(PiModel::Pi5));

        // All and None
        assert!(ConfigFilter::All.matches(PiModel::Pi5));
        assert!(ConfigFilter::All.matches(PiModel::Pi4));
        assert!(!ConfigFilter::None.matches(PiModel::Pi5));
        assert!(!ConfigFilter::None.matches(PiModel::Pi4));
    }

    #[test]
    fn test_should_apply_filter_inheritance() {
        // Pi500 should apply both [pi500] and [pi5] settings
        assert!(should_apply_filter(ConfigFilter::Pi500, PiModel::Pi500));
        assert!(should_apply_filter(ConfigFilter::Pi5, PiModel::Pi500));
        assert!(!should_apply_filter(ConfigFilter::Pi4, PiModel::Pi500));

        // Pi400 should apply both [pi400] and [pi4] settings
        assert!(should_apply_filter(ConfigFilter::Pi400, PiModel::Pi400));
        assert!(should_apply_filter(ConfigFilter::Pi4, PiModel::Pi400));
        assert!(!should_apply_filter(ConfigFilter::Pi5, PiModel::Pi400));

        // Pi5 should only apply [pi5], not [pi500]
        assert!(should_apply_filter(ConfigFilter::Pi5, PiModel::Pi5));
        assert!(!should_apply_filter(ConfigFilter::Pi500, PiModel::Pi5));
    }

    #[test]
    fn test_parse_config_respects_pi5_filter() {
        let config_content = r#"
# Global settings
gpu_freq=900

[pi4]
arm_freq=2000
over_voltage=4

[pi5]
arm_freq=2800
over_voltage_delta=50000

[all]
force_turbo=0
"#;

        // Parse for Pi 5
        let config = parse_config_from_str(config_content, PiModel::Pi5);
        assert_eq!(config.arm_freq, Some(2800)); // From [pi5]
        assert_eq!(config.over_voltage_delta, Some(50000)); // From [pi5]
        assert_eq!(config.over_voltage, None); // NOT from [pi4]
        assert_eq!(config.gpu_freq, Some(900)); // Global
        assert_eq!(config.force_turbo, Some(0)); // From [all]
    }

    #[test]
    fn test_parse_config_respects_pi4_filter() {
        let config_content = r#"
# Global settings
gpu_freq=900

[pi4]
arm_freq=2000
over_voltage=4

[pi5]
arm_freq=2800
over_voltage_delta=50000

[all]
force_turbo=0
"#;

        // Parse for Pi 4
        let config = parse_config_from_str(config_content, PiModel::Pi4);
        assert_eq!(config.arm_freq, Some(2000)); // From [pi4]
        assert_eq!(config.over_voltage, Some(4)); // From [pi4]
        assert_eq!(config.over_voltage_delta, None); // NOT from [pi5]
        assert_eq!(config.gpu_freq, Some(900)); // Global
        assert_eq!(config.force_turbo, Some(0)); // From [all]
    }

    #[test]
    fn test_parse_config_pi500_inherits_from_pi5() {
        let config_content = r#"
[pi5]
arm_freq=2800
gpu_freq=1000

[pi500]
arm_freq=3000
"#;

        // Pi500 should read from both [pi5] and [pi500]
        // Later values override earlier ones
        let config = parse_config_from_str(config_content, PiModel::Pi500);
        assert_eq!(config.arm_freq, Some(3000)); // Overridden by [pi500]
        assert_eq!(config.gpu_freq, Some(1000)); // Inherited from [pi5]
    }

    #[test]
    fn test_parse_config_none_filter() {
        let config_content = r#"
arm_freq=2400

[none]
# These settings are disabled
arm_freq=9999
over_voltage=99

[all]
gpu_freq=900
"#;

        let config = parse_config_from_str(config_content, PiModel::Pi5);
        assert_eq!(config.arm_freq, Some(2400)); // From global, [none] ignored
        assert_eq!(config.over_voltage, None); // [none] ignored
        assert_eq!(config.gpu_freq, Some(900)); // From [all]
    }

    #[test]
    fn test_parse_config_unknown_filter_preserves_state() {
        let config_content = r#"
[pi5]
arm_freq=2800

[EDID=something]
# Unknown filter, previous [pi5] state preserved
gpu_freq=1000

[all]
force_turbo=0
"#;

        let config = parse_config_from_str(config_content, PiModel::Pi5);
        assert_eq!(config.arm_freq, Some(2800)); // From [pi5]
        assert_eq!(config.gpu_freq, Some(1000)); // Applied because [pi5] state preserved
        assert_eq!(config.force_turbo, Some(0)); // From [all]
    }
}
