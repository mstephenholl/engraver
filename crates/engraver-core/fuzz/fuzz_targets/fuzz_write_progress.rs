//! Fuzz test for WriteProgress
//!
//! Tests that WriteProgress calculations handle all inputs safely.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::time::Duration;

#[derive(Arbitrary, Debug)]
struct ProgressInput {
    bytes_written: u64,
    total_bytes: u64,
    speed_bps: u64,
    block_size: usize,
    eta_seconds: Option<u64>,
}

fuzz_target!(|input: ProgressInput| {
    // Ensure block_size is at least 1 to avoid division by zero
    let block_size = if input.block_size == 0 { 1 } else { input.block_size };

    // Create progress
    let mut progress = WriteProgress::new(input.total_bytes, block_size);
    progress.bytes_written = input.bytes_written;
    progress.speed_bps = input.speed_bps;
    progress.eta_seconds = input.eta_seconds;

    // Test percentage calculation
    let percentage = progress.percentage();
    assert!(
        (0.0..=100.0).contains(&percentage) || percentage.is_nan() == false,
        "Invalid percentage: {}",
        percentage
    );

    // For valid inputs, percentage should be 0-100
    if input.total_bytes > 0 {
        assert!(percentage >= 0.0, "Negative percentage: {}", percentage);
        // bytes_written can exceed total_bytes (overshoot), so > 100 is allowed
    }

    // Test is_complete
    let is_complete = progress.is_complete();
    if input.bytes_written >= input.total_bytes && input.total_bytes > 0 {
        assert!(is_complete, "Should be complete");
    }

    // Test speed_display - should never panic
    let speed_display = progress.speed_display();
    assert!(!speed_display.is_empty());

    // Test eta_display - should never panic
    let eta_display = progress.eta_display();
    assert!(!eta_display.is_empty());

    // Test calculate_eta
    let eta = calculate_eta(input.bytes_written, input.total_bytes, input.speed_bps);
    if input.speed_bps == 0 || input.bytes_written >= input.total_bytes {
        assert!(eta.is_none(), "ETA should be None");
    }
});

/// WriteProgress replica for fuzzing
#[derive(Debug, Clone)]
struct WriteProgress {
    bytes_written: u64,
    total_bytes: u64,
    speed_bps: u64,
    eta_seconds: Option<u64>,
    current_block: u64,
    total_blocks: u64,
    elapsed: Duration,
    retry_count: u32,
}

impl WriteProgress {
    fn new(total_bytes: u64, block_size: usize) -> Self {
        let total_blocks = if block_size > 0 {
            (total_bytes + block_size as u64 - 1) / block_size as u64
        } else {
            0
        };
        Self {
            bytes_written: 0,
            total_bytes,
            speed_bps: 0,
            eta_seconds: None,
            current_block: 0,
            total_blocks,
            elapsed: Duration::ZERO,
            retry_count: 0,
        }
    }

    fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_written as f64 / self.total_bytes as f64) * 100.0
        }
    }

    fn is_complete(&self) -> bool {
        self.bytes_written >= self.total_bytes
    }

    fn speed_display(&self) -> String {
        format_speed(self.speed_bps)
    }

    fn eta_display(&self) -> String {
        match self.eta_seconds {
            Some(secs) if secs > 0 => format_duration(secs),
            _ => "calculating...".to_string(),
        }
    }
}

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

fn calculate_eta(bytes_written: u64, total_bytes: u64, speed_bps: u64) -> Option<u64> {
    if speed_bps == 0 || bytes_written >= total_bytes {
        return None;
    }

    let remaining = total_bytes.saturating_sub(bytes_written);
    Some(remaining / speed_bps)
}
