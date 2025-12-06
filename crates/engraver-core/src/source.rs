//! Source handling for Engraver
//!
//! This module handles reading from various source types:
//! - Local files (ISO, IMG, raw)
//! - Remote URLs (HTTP/HTTPS)
//! - Compressed files (gzip, xz, zstd, bzip2)
//!
//! TODO: Full implementation in next phase

use crate::error::{Error, Result};
use std::io::Read;
use std::path::Path;

/// Source type enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum SourceType {
    /// Local file
    LocalFile,
    /// HTTP/HTTPS URL
    Remote,
    /// Gzip compressed
    Gzip,
    /// XZ compressed
    Xz,
    /// Zstd compressed
    Zstd,
    /// Bzip2 compressed
    Bzip2,
}

/// Detect source type from path or URL
pub fn detect_source_type(path: &str) -> SourceType {
    if path.starts_with("http://") || path.starts_with("https://") {
        return SourceType::Remote;
    }

    let lower = path.to_lowercase();
    if lower.ends_with(".gz") || lower.ends_with(".gzip") {
        SourceType::Gzip
    } else if lower.ends_with(".xz") {
        SourceType::Xz
    } else if lower.ends_with(".zst") || lower.ends_with(".zstd") {
        SourceType::Zstd
    } else if lower.ends_with(".bz2") || lower.ends_with(".bzip2") {
        SourceType::Bzip2
    } else {
        SourceType::LocalFile
    }
}

/// Get the uncompressed size of a source (if known)
pub fn get_source_size(path: &str) -> Result<Option<u64>> {
    let source_type = detect_source_type(path);

    match source_type {
        SourceType::LocalFile => {
            let metadata = std::fs::metadata(path)
                .map_err(|_| Error::SourceNotFound(path.to_string()))?;
            Ok(Some(metadata.len()))
        }
        SourceType::Remote => {
            // Would need HEAD request - return None for now
            Ok(None)
        }
        _ => {
            // Compressed - size unknown without reading
            Ok(None)
        }
    }
}

/// Open a source for reading
pub fn open_source(path: &str) -> Result<Box<dyn Read + Send>> {
    let source_type = detect_source_type(path);

    match source_type {
        SourceType::LocalFile => {
            let file = std::fs::File::open(path)
                .map_err(|_| Error::SourceNotFound(path.to_string()))?;
            Ok(Box::new(file))
        }
        SourceType::Remote => {
            Err(Error::Network("Remote sources not yet implemented".to_string()))
        }
        _ => {
            // For compressed files, we'd wrap with decompressor
            // For now, just open as file
            let file = std::fs::File::open(path)
                .map_err(|_| Error::SourceNotFound(path.to_string()))?;
            Ok(Box::new(file))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_source_type_local() {
        assert_eq!(detect_source_type("/path/to/file.iso"), SourceType::LocalFile);
        assert_eq!(detect_source_type("/path/to/file.img"), SourceType::LocalFile);
        assert_eq!(detect_source_type("file.raw"), SourceType::LocalFile);
    }

    #[test]
    fn test_detect_source_type_remote() {
        assert_eq!(detect_source_type("http://example.com/file.iso"), SourceType::Remote);
        assert_eq!(detect_source_type("https://example.com/file.iso"), SourceType::Remote);
    }

    #[test]
    fn test_detect_source_type_compressed() {
        assert_eq!(detect_source_type("file.iso.gz"), SourceType::Gzip);
        assert_eq!(detect_source_type("file.iso.gzip"), SourceType::Gzip);
        assert_eq!(detect_source_type("file.iso.xz"), SourceType::Xz);
        assert_eq!(detect_source_type("file.iso.zst"), SourceType::Zstd);
        assert_eq!(detect_source_type("file.iso.zstd"), SourceType::Zstd);
        assert_eq!(detect_source_type("file.iso.bz2"), SourceType::Bzip2);
    }

    #[test]
    fn test_detect_source_type_case_insensitive() {
        assert_eq!(detect_source_type("FILE.ISO.GZ"), SourceType::Gzip);
        assert_eq!(detect_source_type("File.Iso.Xz"), SourceType::Xz);
    }
}
