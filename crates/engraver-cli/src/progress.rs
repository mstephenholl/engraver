//! Progress bar utilities for the CLI
//!
//! These utility functions are available for custom progress display
//! implementations but aren't used by the default CLI commands.

/// Format bytes per second for display
#[allow(dead_code)]
pub fn format_speed(bytes_per_sec: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes_per_sec >= GB {
        format!("{:.2} GB/s", bytes_per_sec as f64 / GB as f64)
    } else if bytes_per_sec >= MB {
        format!("{:.2} MB/s", bytes_per_sec as f64 / MB as f64)
    } else if bytes_per_sec >= KB {
        format!("{:.2} KB/s", bytes_per_sec as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

/// Format duration for display
#[allow(dead_code)]
pub fn format_eta(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(500), "500 B/s");
        assert_eq!(format_speed(1024), "1.00 KB/s");
        assert_eq!(format_speed(1024 * 1024), "1.00 MB/s");
        assert_eq!(format_speed(1024 * 1024 * 1024), "1.00 GB/s");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(30), "30s");
        assert_eq!(format_eta(90), "1m 30s");
        assert_eq!(format_eta(3661), "1h 1m");
    }
}
