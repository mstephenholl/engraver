//! Fuzz test for settings TOML parsing
//!
//! Tests that settings deserialization handles arbitrary TOML safely.

#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

fuzz_target!(|data: &str| {
    // Test parsing the full settings structure
    let result: Result<Settings, _> = toml::from_str(data);

    // If parsing succeeded, verify the result can be serialized back
    if let Ok(settings) = result {
        // Should be able to serialize without panicking
        let _ = toml::to_string(&settings);
        let _ = toml::to_string_pretty(&settings);

        // Verify individual fields don't cause issues when accessed
        let _ = settings.write.block_size.len();
        let _ = settings.checksum.algorithm.len();
        let _ = settings.benchmark.pattern.len();
        let _ = settings.benchmark.test_size.len();
    }

    // Test parsing individual sections
    let _: Result<WriteSettings, _> = toml::from_str(data);
    let _: Result<ChecksumSettings, _> = toml::from_str(data);
    let _: Result<BehaviorSettings, _> = toml::from_str(data);
    let _: Result<BenchmarkSettings, _> = toml::from_str(data);

    // Test with table wrappers (how they appear in full config)
    let wrapped = format!("[write]\n{}", data);
    let _: Result<Settings, _> = toml::from_str(&wrapped);

    let wrapped = format!("[checksum]\n{}", data);
    let _: Result<Settings, _> = toml::from_str(&wrapped);

    let wrapped = format!("[benchmark]\n{}", data);
    let _: Result<Settings, _> = toml::from_str(&wrapped);
});

/// User settings structure (mirrors engraver_core::Settings)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub write: WriteSettings,
    pub checksum: ChecksumSettings,
    pub behavior: BehaviorSettings,
    pub benchmark: BenchmarkSettings,
}

/// Settings for write operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WriteSettings {
    pub block_size: String,
    pub verify: bool,
    pub checkpoint: bool,
}

impl Default for WriteSettings {
    fn default() -> Self {
        Self {
            block_size: "4M".to_string(),
            verify: false,
            checkpoint: false,
        }
    }
}

/// Settings for checksum operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChecksumSettings {
    pub algorithm: String,
    pub auto_detect: bool,
}

impl Default for ChecksumSettings {
    fn default() -> Self {
        Self {
            algorithm: "sha256".to_string(),
            auto_detect: false,
        }
    }
}

/// General behavior settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorSettings {
    pub skip_confirmation: bool,
    pub quiet: bool,
}

/// Settings for benchmark operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BenchmarkSettings {
    pub block_size: String,
    pub test_size: String,
    pub pattern: String,
    pub passes: u32,
    pub json: bool,
}

impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            block_size: "4M".to_string(),
            test_size: "256M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
        }
    }
}
