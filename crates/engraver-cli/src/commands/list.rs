//! List command implementation

use anyhow::Result;
use console::style;

pub fn execute(show_all: bool) -> Result<()> {
    let drives = engraver_detect::list_removable_drives()?;

    if drives.is_empty() {
        println!("No removable drives found.");
        return Ok(());
    }

    println!(
        "{} {} drive(s):\n",
        style("Found").green().bold(),
        drives.len()
    );

    for drive in &drives {
        if !show_all && !drive.is_safe_target() {
            continue;
        }

        println!(
            "{} - {} ({})",
            style(&drive.path).white().bold(),
            drive.name,
            drive.size_display()
        );
    }

    Ok(())
}
