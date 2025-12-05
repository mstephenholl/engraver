//! Checksum command implementation

use anyhow::Result;

pub fn execute(path: String, algorithm: String) -> Result<()> {
    println!("Calculating {} checksum of {}", algorithm, path);
    todo!("Implement checksum command")
}
