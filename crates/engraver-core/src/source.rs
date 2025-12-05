//! Source handling for local, remote, and compressed images

use crate::Result;
use std::path::Path;

/// Represents an image source that can be read
pub trait ImageSource: Send + Sync {
    /// Get the total size of the source in bytes
    fn size(&self) -> Result<u64>;

    /// Read a block of data from the source
    fn read_block(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize>;

    /// Get the source type for display
    fn source_type(&self) -> SourceType;
}

/// Type of image source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// Local file
    Local,
    /// Remote HTTP/HTTPS URL
    Remote,
    /// Compressed archive
    Compressed,
}

/// Detect and open an image source from a path or URL
pub fn open_source(_path_or_url: &str) -> Result<Box<dyn ImageSource>> {
    todo!("Implement source detection and opening")
}

/// Check if a path is a supported compressed format
pub fn is_compressed(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext.to_lowercase().as_str(), "gz" | "xz" | "zst" | "bz2" | "zip")
}
