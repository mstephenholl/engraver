//! Engraver CLI - Create bootable USB drives from ISO images

use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("engraver=info".parse().unwrap()),
        )
        .init();

    let cli = cli::Cli::parse();
    cli.execute()
}
