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
