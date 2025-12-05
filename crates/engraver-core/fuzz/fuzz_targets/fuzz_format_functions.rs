//! Fuzz test for format functions
//!
//! Tests that format_speed and format_duration handle all inputs safely.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct FormatInput {
    speed: u64,
    duration: u64,
}

fuzz_target!(|input: FormatInput| {
    // Test format_speed
    let speed_str = format_speed(input.speed);

    // Should never panic
    // Should always return non-empty string
    assert!(!speed_str.is_empty());

    // Should contain a space before unit
    if input.speed > 0 {
        assert!(speed_str.contains(' '), "Missing space in: {}", speed_str);
    }

    // Should end with /s
    assert!(speed_str.ends_with("/s"), "Missing /s suffix: {}", speed_str);

    // Test format_duration
    let duration_str = format_duration(input.duration);

    // Should never panic
    assert!(!duration_str.is_empty());

    // Should contain time unit
    assert!(
        duration_str.contains('s') || duration_str.contains('m') || duration_str.contains('h'),
        "Missing time unit: {}",
        duration_str
    );
});

/// Format speed (copy for fuzzing)
fn format_speed(bytes_per_second: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes_per_second >= GB {
        format!("{:.1} GB/s", bytes_per_second as f64 / GB as f64)
    } else if bytes_per_second >= MB {
        format!("{:.1} MB/s", bytes_per_second as f64 / MB as f64)
    } else if bytes_per_second >= KB {
        format!("{:.1} KB/s", bytes_per_second as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_second)
    }
}

/// Format duration (copy for fuzzing)
fn format_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{}h {}m", hours, mins)
    } else if seconds >= 60 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", seconds)
    }
}
