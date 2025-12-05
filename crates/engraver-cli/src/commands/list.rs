//! List command implementation

use anyhow::Result;

pub fn execute(show_all: bool) -> Result<()> {
    let drives = engraver_detect::list_removable_drives()?;

    if drives.is_empty() {
        println!("No removable drives found.");
        return Ok(());
    }

    for drive in drives {
        if !show_all && !drive.is_safe_target() {
            continue;
        }

        println!(
            "{} - {} ({})",
            drive.path,
            drive.name,
            drive.size_display()
        );
    }

    Ok(())
}
