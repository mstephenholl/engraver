//! List command - displays available drives

use anyhow::Result;
use console::style;

/// Execute the list command
pub fn execute(show_all: bool, json: bool, silent: bool) -> Result<()> {
    let all_drives = engraver_detect::list_drives()?;

    let drives: Vec<_> = if show_all {
        all_drives.clone()
    } else {
        all_drives
            .iter()
            .filter(|d| d.is_safe_target())
            .cloned()
            .collect()
    };

    // JSON output mode - always output even in silent mode (it's machine-readable)
    if json {
        let output = serde_json_drives(&drives);
        println!("{}", output);
        return Ok(());
    }

    // Silent mode - no human-readable output
    if silent {
        return Ok(());
    }

    // Human-readable output
    if drives.is_empty() {
        if show_all {
            println!("No drives found.");
        } else {
            println!("No removable drives found.");
            println!(
                "{}",
                style("Tip: Use --all to show all drives including system drives").dim()
            );
        }
        return Ok(());
    }

    println!(
        "{} {} drive(s):\n",
        style("Found").green().bold(),
        drives.len()
    );

    for drive in &drives {
        print_drive(drive);
    }

    if !show_all {
        let hidden = all_drives.len() - drives.len();
        if hidden > 0 {
            println!(
                "{}",
                style(format!(
                    "Note: {} system/internal drive(s) hidden. Use --all to show.",
                    hidden
                ))
                .dim()
            );
        }
    }

    Ok(())
}

/// Print a single drive's information
fn print_drive(drive: &engraver_detect::Drive) {
    let status = if drive.is_safe_target() {
        style("✓").green().bold()
    } else {
        style("✗").red().bold()
    };

    let removable = if drive.removable {
        style("removable").cyan()
    } else {
        style("internal").yellow()
    };

    println!(
        "{} {} {} ({}, {})",
        status,
        style(&drive.path).white().bold(),
        style(&drive.display_name()).white(),
        drive.size_display(),
        removable
    );

    // Show drive type and USB speed if applicable
    let usb_speed_info = if let Some(speed) = &drive.usb_speed {
        if speed.is_slow() {
            format!(
                " | {} {}",
                style(speed.to_string()).yellow(),
                style("(slow)").yellow().bold()
            )
        } else {
            format!(" | {}", style(speed.to_string()).green())
        }
    } else {
        String::new()
    };

    println!(
        "    Type: {} | {}{}",
        style(drive.drive_type.to_string()).dim(),
        if drive.is_system {
            style("SYSTEM DRIVE").red().bold().to_string()
        } else {
            style("safe target").green().to_string()
        },
        usb_speed_info
    );

    // Show reason if system drive
    if let Some(reason) = &drive.system_reason {
        println!("    Reason: {}", style(reason).dim());
    }

    // Show mount points
    if !drive.mount_points.is_empty() {
        println!(
            "    Mounted: {}",
            style(drive.mount_points.join(", ")).dim()
        );
    }

    // Show partitions
    if !drive.partitions.is_empty() {
        println!("    Partitions:");
        for part in &drive.partitions {
            let label = part.label.as_deref().unwrap_or("(unlabeled)");
            let fs = part.filesystem.as_deref().unwrap_or("unknown");
            let mount = part
                .mount_point
                .as_ref()
                .map(|m| format!(" → {}", m))
                .unwrap_or_default();

            println!(
                "      {} {} [{}]{}",
                style(&part.path).dim(),
                label,
                fs,
                style(mount).cyan()
            );
        }
    }

    println!();
}

/// Simple JSON serialization without serde dependency on Drive
fn serde_json_drives(drives: &[engraver_detect::Drive]) -> String {
    let mut output = String::from("[\n");

    for (i, drive) in drives.iter().enumerate() {
        output.push_str("  {\n");
        output.push_str(&format!(
            "    \"path\": \"{}\",\n",
            escape_json(&drive.path)
        ));
        output.push_str(&format!(
            "    \"vendor\": {},\n",
            opt_json_str(&drive.vendor)
        ));
        output.push_str(&format!("    \"model\": {},\n", opt_json_str(&drive.model)));
        output.push_str(&format!("    \"size\": {},\n", drive.size));
        output.push_str(&format!(
            "    \"size_display\": \"{}\",\n",
            drive.size_display()
        ));
        output.push_str(&format!("    \"removable\": {},\n", drive.removable));
        output.push_str(&format!("    \"is_system\": {},\n", drive.is_system));
        output.push_str(&format!(
            "    \"is_safe_target\": {},\n",
            drive.is_safe_target()
        ));
        output.push_str(&format!("    \"drive_type\": \"{}\",\n", drive.drive_type));
        output.push_str(&format!(
            "    \"usb_speed\": {},\n",
            drive
                .usb_speed
                .as_ref()
                .map_or("null".to_string(), |s| format!("\"{}\"", s))
        ));
        output.push_str(&format!(
            "    \"usb_speed_slow\": {},\n",
            drive.usb_speed.as_ref().is_some_and(|s| s.is_slow())
        ));
        output.push_str(&format!(
            "    \"mount_points\": [{}],\n",
            drive
                .mount_points
                .iter()
                .map(|m| format!("\"{}\"", escape_json(m)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        output.push_str(&format!(
            "    \"partition_count\": {}\n",
            drive.partitions.len()
        ));
        output.push_str("  }");

        if i < drives.len() - 1 {
            output.push(',');
        }
        output.push('\n');
    }

    output.push(']');
    output
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn opt_json_str(opt: &Option<String>) -> String {
    match opt {
        Some(s) => format!("\"{}\"", escape_json(s)),
        None => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engraver_detect::{Drive, DriveType, Partition, UsbSpeed};

    // -------------------------------------------------------------------------
    // escape_json tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_escape_json_no_special_chars() {
        assert_eq!(escape_json("hello world"), "hello world");
        assert_eq!(escape_json("simple"), "simple");
        assert_eq!(escape_json(""), "");
    }

    #[test]
    fn test_escape_json_backslash() {
        assert_eq!(escape_json("path\\to\\file"), "path\\\\to\\\\file");
        assert_eq!(escape_json("\\"), "\\\\");
    }

    #[test]
    fn test_escape_json_quotes() {
        assert_eq!(escape_json("say \"hello\""), "say \\\"hello\\\"");
        assert_eq!(escape_json("\"quoted\""), "\\\"quoted\\\"");
    }

    #[test]
    fn test_escape_json_newlines() {
        assert_eq!(escape_json("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_json("with\r\nwindows"), "with\\r\\nwindows");
    }

    #[test]
    fn test_escape_json_tabs() {
        assert_eq!(escape_json("col1\tcol2"), "col1\\tcol2");
    }

    #[test]
    fn test_escape_json_combined() {
        assert_eq!(
            escape_json("path\\with \"quotes\"\nand\ttabs"),
            "path\\\\with \\\"quotes\\\"\\nand\\ttabs"
        );
    }

    // -------------------------------------------------------------------------
    // opt_json_str tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_opt_json_str_some() {
        assert_eq!(opt_json_str(&Some("value".to_string())), "\"value\"");
        assert_eq!(opt_json_str(&Some("SanDisk".to_string())), "\"SanDisk\"");
    }

    #[test]
    fn test_opt_json_str_none() {
        assert_eq!(opt_json_str(&None), "null");
    }

    #[test]
    fn test_opt_json_str_escapes() {
        assert_eq!(
            opt_json_str(&Some("with \"quotes\"".to_string())),
            "\"with \\\"quotes\\\"\""
        );
    }

    // -------------------------------------------------------------------------
    // serde_json_drives tests
    // -------------------------------------------------------------------------

    fn create_test_drive() -> Drive {
        Drive {
            path: "/dev/sdb".to_string(),
            raw_path: "/dev/sdb".to_string(),
            name: "sdb".to_string(),
            size: 16 * 1024 * 1024 * 1024, // 16 GB
            removable: true,
            drive_type: DriveType::Usb,
            vendor: Some("SanDisk".to_string()),
            model: Some("Ultra USB 3.0".to_string()),
            serial: None,
            partitions: vec![],
            mount_points: vec!["/mnt/usb".to_string()],
            is_system: false,
            system_reason: None,
            usb_speed: Some(UsbSpeed::SuperSpeed),
        }
    }

    #[test]
    fn test_serde_json_drives_empty() {
        let drives: Vec<Drive> = vec![];
        let json = serde_json_drives(&drives);
        assert_eq!(json, "[\n]");
    }

    #[test]
    fn test_serde_json_drives_single() {
        let drives = vec![create_test_drive()];
        let json = serde_json_drives(&drives);

        assert!(json.starts_with("[\n"));
        assert!(json.ends_with("\n]"));
        assert!(json.contains("\"path\": \"/dev/sdb\""));
        assert!(json.contains("\"vendor\": \"SanDisk\""));
        assert!(json.contains("\"model\": \"Ultra USB 3.0\""));
        assert!(json.contains("\"size\": 17179869184"));
        assert!(json.contains("\"removable\": true"));
        assert!(json.contains("\"is_system\": false"));
        assert!(json.contains("\"is_safe_target\": true"));
        assert!(json.contains("\"drive_type\": \"USB\""));
        assert!(json.contains("\"usb_speed\": \"USB 3.0 (5 Gbps)\""));
        assert!(json.contains("\"mount_points\": [\"/mnt/usb\"]"));
    }

    #[test]
    fn test_serde_json_drives_multiple() {
        let drive1 = create_test_drive();
        let mut drive2 = create_test_drive();
        drive2.path = "/dev/sdc".to_string();
        drive2.vendor = None;

        let drives = vec![drive1, drive2];
        let json = serde_json_drives(&drives);

        // Should have comma between objects
        assert!(json.contains("},\n"));
        // Should have both paths
        assert!(json.contains("\"/dev/sdb\""));
        assert!(json.contains("\"/dev/sdc\""));
        // Check null handling
        assert!(json.contains("\"vendor\": \"SanDisk\""));
        assert!(json.contains("\"vendor\": null"));
    }

    #[test]
    fn test_serde_json_drives_with_partitions() {
        let mut drive = create_test_drive();
        drive.partitions = vec![
            Partition {
                path: "/dev/sdb1".to_string(),
                size: 8 * 1024 * 1024 * 1024,
                filesystem: Some("vfat".to_string()),
                label: Some("BOOT".to_string()),
                mount_point: Some("/boot/efi".to_string()),
            },
            Partition {
                path: "/dev/sdb2".to_string(),
                size: 8 * 1024 * 1024 * 1024,
                filesystem: Some("ext4".to_string()),
                label: None,
                mount_point: None,
            },
        ];

        let drives = vec![drive];
        let json = serde_json_drives(&drives);

        assert!(json.contains("\"partition_count\": 2"));
    }

    #[test]
    fn test_serde_json_drives_escaping() {
        let mut drive = create_test_drive();
        drive.model = Some("Model \"with\" quotes".to_string());

        let drives = vec![drive];
        let json = serde_json_drives(&drives);

        assert!(json.contains("Model \\\"with\\\" quotes"));
    }

    #[test]
    fn test_serde_json_drives_slow_usb() {
        let mut drive = create_test_drive();
        drive.usb_speed = Some(UsbSpeed::High); // USB 2.0 High Speed

        let drives = vec![drive];
        let json = serde_json_drives(&drives);

        // High speed (USB 2.0) is considered slow for USB 3.0 capable devices
        assert!(json.contains("\"usb_speed\":"));
        assert!(json.contains("\"usb_speed_slow\": true"));
    }

    #[test]
    fn test_serde_json_drives_no_usb_speed() {
        let mut drive = create_test_drive();
        drive.usb_speed = None;

        let drives = vec![drive];
        let json = serde_json_drives(&drives);

        assert!(json.contains("\"usb_speed\": null"));
        assert!(json.contains("\"usb_speed_slow\": false"));
    }
}
