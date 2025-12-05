//! Verify command implementation

use anyhow::Result;

pub fn execute(source: String, target: String) -> Result<()> {
    println!("Verifying {} against {}", target, source);
    todo!("Implement verify command")
}
