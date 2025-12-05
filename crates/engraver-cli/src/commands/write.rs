//! Write command implementation

use anyhow::Result;
use console::style;

pub fn execute(
    source: String,
    target: String,
    verify: bool,
    skip_confirm: bool,
    _block_size: String,
) -> Result<()> {
    println!(
        "{} {} → {}",
        style("Writing").cyan().bold(),
        style(&source).green(),
        style(&target).yellow()
    );

    if !skip_confirm {
        println!(
            "\n{} This will ERASE ALL DATA on {}!",
            style("WARNING:").red().bold(),
            style(&target).yellow()
        );
        // TODO: Add confirmation prompt
    }

    if verify {
        println!("Verification: enabled");
    }

    todo!("Implement write command")
}
