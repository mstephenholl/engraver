//! Source handling for Engraver
//!
//! This module handles reading from various source types:
//! - Local files (ISO, IMG, raw)
//! - Remote URLs (HTTP/HTTPS) with resume support
//! - Compressed files (gzip, xz, zstd, bzip2)
//!
//! ## Example
//!
//! ```no_run
//! use engraver_core::source::{Source, SourceInfo};
//!
//! // Open a local file
//! let source = Source::open("image.iso")?;
//! println!("Size: {:?}", source.info().size);
//!
//! // Open a compressed file (auto-detected)
//! let source = Source::open("image.iso.gz")?;
//!
//! // Open a remote URL
//! let source = Source::open("https://example.com/image.iso")?;
//! # Ok::<(), engraver_core::Error>(())
//! ```

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

// ============================================================================
// Source Types and Detection
// ============================================================================

/// Source type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    /// Local uncompressed file
    LocalFile,
    /// HTTP/HTTPS URL
    Remote,
    /// Gzip compressed (.gz)
    Gzip,
    /// XZ compressed (.xz)
    Xz,
    /// Zstandard compressed (.zst)
    Zstd,
    /// Bzip2 compressed (.bz2)
    Bzip2,
}

impl SourceType {
    /// Check if this source type is compressed
    pub fn is_compressed(&self) -> bool {
        matches!(
            self,
            SourceType::Gzip | SourceType::Xz | SourceType::Zstd | SourceType::Bzip2
        )
    }

    /// Check if this source type is remote
    pub fn is_remote(&self) -> bool {
        matches!(self, SourceType::Remote)
    }

    /// Get the compression extension
    pub fn extension(&self) -> Option<&'static str> {
        match self {
            SourceType::Gzip => Some(".gz"),
            SourceType::Xz => Some(".xz"),
            SourceType::Zstd => Some(".zst"),
            SourceType::Bzip2 => Some(".bz2"),
            _ => None,
        }
    }
}

/// Detect source type from path or URL
pub fn detect_source_type(path: &str) -> SourceType {
    // Check for remote URLs first
    if path.starts_with("http://") || path.starts_with("https://") {
        return SourceType::Remote;
    }

    // Check compression by extension
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

/// Detect compression type from magic bytes
pub fn detect_compression_from_magic(bytes: &[u8]) -> Option<SourceType> {
    if bytes.len() < 6 {
        return None;
    }

    // Gzip: 1f 8b
    if bytes[0] == 0x1f && bytes[1] == 0x8b {
        return Some(SourceType::Gzip);
    }

    // XZ: fd 37 7a 58 5a 00
    if bytes.len() >= 6
        && bytes[0] == 0xfd
        && bytes[1] == 0x37
        && bytes[2] == 0x7a
        && bytes[3] == 0x58
        && bytes[4] == 0x5a
        && bytes[5] == 0x00
    {
        return Some(SourceType::Xz);
    }

    // Zstd: 28 b5 2f fd
    if bytes.len() >= 4
        && bytes[0] == 0x28
        && bytes[1] == 0xb5
        && bytes[2] == 0x2f
        && bytes[3] == 0xfd
    {
        return Some(SourceType::Zstd);
    }

    // Bzip2: 42 5a 68 (BZh)
    if bytes.len() >= 3 && bytes[0] == 0x42 && bytes[1] == 0x5a && bytes[2] == 0x68 {
        return Some(SourceType::Bzip2);
    }

    None
}

// ============================================================================
// Source Information
// ============================================================================

/// Information about a source
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// Original path or URL
    pub path: String,

    /// Detected source type
    pub source_type: SourceType,

    /// Compressed size (if known)
    pub compressed_size: Option<u64>,

    /// Uncompressed size (if known)
    pub size: Option<u64>,

    /// Whether the source supports seeking
    pub seekable: bool,

    /// Whether the source supports resuming (for HTTP)
    pub resumable: bool,

    /// Content type (for HTTP sources)
    pub content_type: Option<String>,

    /// ETag (for HTTP sources, used for resume validation)
    pub etag: Option<String>,
}

impl SourceInfo {
    /// Create info for a local file
    pub fn local(path: &str, size: u64) -> Self {
        Self {
            path: path.to_string(),
            source_type: detect_source_type(path),
            compressed_size: Some(size),
            size: Some(size),
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
        }
    }

    /// Create info for a compressed file
    pub fn compressed(path: &str, compressed_size: u64, source_type: SourceType) -> Self {
        Self {
            path: path.to_string(),
            source_type,
            compressed_size: Some(compressed_size),
            size: None, // Unknown until decompressed
            seekable: false,
            resumable: false,
            content_type: None,
            etag: None,
        }
    }
}

// ============================================================================
// Local File Source
// ============================================================================

/// A local file source
pub struct LocalFileSource {
    file: BufReader<File>,
    info: SourceInfo,
}

impl LocalFileSource {
    /// Open a local file
    pub fn open(path: &str) -> Result<Self> {
        let file_path = Path::new(path);

        if !file_path.exists() {
            return Err(Error::SourceNotFound(path.to_string()));
        }

        let file = File::open(file_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Error::PermissionDenied(format!("Cannot read {}: {}", path, e))
            } else {
                Error::Io(e)
            }
        })?;

        let metadata = file.metadata()?;
        let size = metadata.len();

        let info = SourceInfo::local(path, size);

        Ok(Self {
            file: BufReader::with_capacity(64 * 1024, file),
            info,
        })
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }
}

impl Read for LocalFileSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for LocalFileSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}

// ============================================================================
// Compressed Source Wrappers
// ============================================================================

/// Wrapper for gzip-compressed sources
#[cfg(feature = "compression")]
pub struct GzipSource<R: Read> {
    decoder: flate2::read::GzDecoder<R>,
    info: SourceInfo,
}

#[cfg(feature = "compression")]
impl<R: Read> GzipSource<R> {
    /// Create a new gzip source
    pub fn new(reader: R, info: SourceInfo) -> Self {
        Self {
            decoder: flate2::read::GzDecoder::new(reader),
            info,
        }
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }
}

#[cfg(feature = "compression")]
impl<R: Read> Read for GzipSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.decoder.read(buf)
    }
}

/// Wrapper for xz-compressed sources
#[cfg(feature = "compression")]
pub struct XzSource<R: Read> {
    decoder: xz2::read::XzDecoder<R>,
    info: SourceInfo,
}

#[cfg(feature = "compression")]
impl<R: Read> XzSource<R> {
    /// Create a new xz source
    pub fn new(reader: R, info: SourceInfo) -> Self {
        Self {
            decoder: xz2::read::XzDecoder::new(reader),
            info,
        }
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }
}

#[cfg(feature = "compression")]
impl<R: Read> Read for XzSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.decoder.read(buf)
    }
}

/// Wrapper for zstd-compressed sources
#[cfg(feature = "compression")]
pub struct ZstdSource<'a, R: Read> {
    decoder: zstd::Decoder<'a, BufReader<R>>,
    info: SourceInfo,
}

#[cfg(feature = "compression")]
impl<'a, R: Read> ZstdSource<'a, R> {
    /// Create a new zstd source
    pub fn new(reader: R, info: SourceInfo) -> Result<Self> {
        let decoder = zstd::Decoder::new(reader)
            .map_err(|e| Error::Decompression(format!("Failed to create zstd decoder: {}", e)))?;
        Ok(Self { decoder, info })
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }
}

#[cfg(feature = "compression")]
impl<R: Read> Read for ZstdSource<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.decoder.read(buf)
    }
}

/// Wrapper for bzip2-compressed sources
#[cfg(feature = "compression")]
pub struct Bzip2Source<R: Read> {
    decoder: bzip2::read::BzDecoder<R>,
    info: SourceInfo,
}

#[cfg(feature = "compression")]
impl<R: Read> Bzip2Source<R> {
    /// Create a new bzip2 source
    pub fn new(reader: R, info: SourceInfo) -> Self {
        Self {
            decoder: bzip2::read::BzDecoder::new(reader),
            info,
        }
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }
}

#[cfg(feature = "compression")]
impl<R: Read> Read for Bzip2Source<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.decoder.read(buf)
    }
}

// ============================================================================
// HTTP/HTTPS Source
// ============================================================================

/// HTTP source with resume support
#[cfg(feature = "remote")]
pub struct HttpSource {
    response: reqwest::blocking::Response,
    info: SourceInfo,
    bytes_read: u64,
}

#[cfg(feature = "remote")]
impl HttpSource {
    /// Open an HTTP/HTTPS URL
    pub fn open(url: &str) -> Result<Self> {
        Self::open_with_resume(url, 0)
    }

    /// Open an HTTP/HTTPS URL with resume from a specific offset
    pub fn open_with_resume(url: &str, offset: u64) -> Result<Self> {
        // Validate URL
        let parsed_url = url::Url::parse(url)
            .map_err(|e| Error::Network(format!("Invalid URL '{}': {}", url, e)))?;

        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(Error::Network(format!(
                "Unsupported URL scheme: {}",
                parsed_url.scheme()
            )));
        }

        // Build request
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("engraver/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Network(format!("Failed to create HTTP client: {}", e)))?;

        let mut request = client.get(url);

        // Add Range header for resume
        if offset > 0 {
            request = request.header("Range", format!("bytes={}-", offset));
        }

        // Send request
        let response = request
            .send()
            .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

        // Check status
        let status = response.status();
        if !status.is_success() && status.as_u16() != 206 {
            return Err(Error::Network(format!(
                "HTTP error {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Extract headers
        let content_length = response.content_length();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let accept_ranges = response
            .headers()
            .get("accept-ranges")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "bytes")
            .unwrap_or(false);

        // Calculate total size (accounting for resume)
        let total_size = if offset > 0 && status.as_u16() == 206 {
            content_length.map(|cl| cl + offset)
        } else {
            content_length
        };

        let info = SourceInfo {
            path: url.to_string(),
            source_type: SourceType::Remote,
            compressed_size: total_size,
            size: total_size,
            seekable: false,
            resumable: accept_ranges,
            content_type,
            etag,
        };

        Ok(Self {
            response,
            info,
            bytes_read: offset,
        })
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }

    /// Get bytes read so far
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Check if this source supports resume
    pub fn supports_resume(&self) -> bool {
        self.info.resumable
    }
}

#[cfg(feature = "remote")]
impl Read for HttpSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.response.read(buf)?;
        self.bytes_read += n as u64;
        Ok(n)
    }
}

// ============================================================================
// Unified Source Interface
// ============================================================================

/// Unified source that can read from any supported source type
pub enum Source {
    /// Local uncompressed file
    Local(LocalFileSource),

    /// Gzip compressed local file
    #[cfg(feature = "compression")]
    Gzip(GzipSource<BufReader<File>>),

    /// XZ compressed local file
    #[cfg(feature = "compression")]
    Xz(XzSource<BufReader<File>>),

    /// Zstd compressed local file
    #[cfg(feature = "compression")]
    Zstd(Box<ZstdSource<'static, BufReader<File>>>),

    /// Bzip2 compressed local file
    #[cfg(feature = "compression")]
    Bzip2(Bzip2Source<BufReader<File>>),

    /// HTTP/HTTPS remote source
    #[cfg(feature = "remote")]
    Http(HttpSource),

    /// HTTP source with gzip compression
    #[cfg(all(feature = "remote", feature = "compression"))]
    HttpGzip(GzipSource<HttpSource>),

    /// HTTP source with xz compression
    #[cfg(all(feature = "remote", feature = "compression"))]
    HttpXz(XzSource<HttpSource>),

    /// HTTP source with zstd compression
    #[cfg(all(feature = "remote", feature = "compression"))]
    HttpZstd(Box<ZstdSource<'static, HttpSource>>),

    /// HTTP source with bzip2 compression
    #[cfg(all(feature = "remote", feature = "compression"))]
    HttpBzip2(Bzip2Source<HttpSource>),
}

impl Source {
    /// Open a source from a path or URL
    ///
    /// Automatically detects the source type and compression.
    pub fn open(path: &str) -> Result<Self> {
        Self::open_with_offset(path, 0)
    }

    /// Open a source from a path or URL, seeking to the specified offset
    ///
    /// This is useful for resuming interrupted writes. For local files, this
    /// seeks to the offset. For HTTP sources, this uses Range headers.
    /// Compressed sources cannot be resumed (returns error if offset > 0).
    pub fn open_with_offset(path: &str, offset: u64) -> Result<Self> {
        let source_type = detect_source_type(path);

        match source_type {
            SourceType::LocalFile => {
                let mut source = LocalFileSource::open(path)?;
                if offset > 0 {
                    source.seek(SeekFrom::Start(offset))?;
                }
                Ok(Source::Local(source))
            }

            #[cfg(feature = "compression")]
            SourceType::Gzip => {
                if offset > 0 {
                    return Err(Error::InvalidConfig(
                        "Cannot resume from compressed gzip source".to_string(),
                    ));
                }
                let file = open_file_buffered(path)?;
                let compressed_size = file.get_ref().metadata()?.len();
                let info = SourceInfo::compressed(path, compressed_size, SourceType::Gzip);
                Ok(Source::Gzip(GzipSource::new(file, info)))
            }

            #[cfg(feature = "compression")]
            SourceType::Xz => {
                if offset > 0 {
                    return Err(Error::InvalidConfig(
                        "Cannot resume from compressed xz source".to_string(),
                    ));
                }
                let file = open_file_buffered(path)?;
                let compressed_size = file.get_ref().metadata()?.len();
                let info = SourceInfo::compressed(path, compressed_size, SourceType::Xz);
                Ok(Source::Xz(XzSource::new(file, info)))
            }

            #[cfg(feature = "compression")]
            SourceType::Zstd => {
                if offset > 0 {
                    return Err(Error::InvalidConfig(
                        "Cannot resume from compressed zstd source".to_string(),
                    ));
                }
                let file = open_file_buffered(path)?;
                let compressed_size = file.get_ref().metadata()?.len();
                let info = SourceInfo::compressed(path, compressed_size, SourceType::Zstd);
                Ok(Source::Zstd(Box::new(ZstdSource::new(file, info)?)))
            }

            #[cfg(feature = "compression")]
            SourceType::Bzip2 => {
                if offset > 0 {
                    return Err(Error::InvalidConfig(
                        "Cannot resume from compressed bzip2 source".to_string(),
                    ));
                }
                let file = open_file_buffered(path)?;
                let compressed_size = file.get_ref().metadata()?.len();
                let info = SourceInfo::compressed(path, compressed_size, SourceType::Bzip2);
                Ok(Source::Bzip2(Bzip2Source::new(file, info)))
            }

            #[cfg(feature = "remote")]
            SourceType::Remote => {
                let http_source = HttpSource::open_with_resume(path, offset)?;
                Ok(Source::Http(http_source))
            }

            #[cfg(not(feature = "compression"))]
            SourceType::Gzip | SourceType::Xz | SourceType::Zstd | SourceType::Bzip2 => {
                if offset > 0 {
                    return Err(Error::InvalidConfig(
                        "Cannot resume from compressed source".to_string(),
                    ));
                }
                Err(Error::InvalidConfig(
                    "Compression support not enabled. Rebuild with 'compression' feature."
                        .to_string(),
                ))
            }

            #[cfg(not(feature = "remote"))]
            SourceType::Remote => Err(Error::InvalidConfig(
                "Remote source support not enabled. Rebuild with 'remote' feature.".to_string(),
            )),
        }
    }

    /// Get source information
    pub fn info(&self) -> &SourceInfo {
        match self {
            Source::Local(s) => s.info(),
            #[cfg(feature = "compression")]
            Source::Gzip(s) => s.info(),
            #[cfg(feature = "compression")]
            Source::Xz(s) => s.info(),
            #[cfg(feature = "compression")]
            Source::Zstd(s) => s.info(),
            #[cfg(feature = "compression")]
            Source::Bzip2(s) => s.info(),
            #[cfg(feature = "remote")]
            Source::Http(s) => s.info(),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpGzip(s) => s.info(),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpXz(s) => s.info(),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpZstd(s) => s.info(),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpBzip2(s) => s.info(),
        }
    }

    /// Get the known size (uncompressed if available, otherwise compressed)
    pub fn size(&self) -> Option<u64> {
        let info = self.info();
        info.size.or(info.compressed_size)
    }

    /// Check if this source is seekable
    pub fn is_seekable(&self) -> bool {
        self.info().seekable
    }

    /// Check if this source is compressed
    pub fn is_compressed(&self) -> bool {
        self.info().source_type.is_compressed()
    }
}

impl Read for Source {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Source::Local(s) => s.read(buf),
            #[cfg(feature = "compression")]
            Source::Gzip(s) => s.read(buf),
            #[cfg(feature = "compression")]
            Source::Xz(s) => s.read(buf),
            #[cfg(feature = "compression")]
            Source::Zstd(s) => s.read(buf),
            #[cfg(feature = "compression")]
            Source::Bzip2(s) => s.read(buf),
            #[cfg(feature = "remote")]
            Source::Http(s) => s.read(buf),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpGzip(s) => s.read(buf),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpXz(s) => s.read(buf),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpZstd(s) => s.read(buf),
            #[cfg(all(feature = "remote", feature = "compression"))]
            Source::HttpBzip2(s) => s.read(buf),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Open a file with buffered reading
fn open_file_buffered(path: &str) -> Result<BufReader<File>> {
    let file = File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::SourceNotFound(path.to_string())
        } else if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::PermissionDenied(format!("Cannot read {}: {}", path, e))
        } else {
            Error::Io(e)
        }
    })?;

    Ok(BufReader::with_capacity(64 * 1024, file))
}

/// Get the size of a source without opening it for reading
pub fn get_source_size(path: &str) -> Result<Option<u64>> {
    let source_type = detect_source_type(path);

    match source_type {
        SourceType::LocalFile => {
            let metadata =
                std::fs::metadata(path).map_err(|_| Error::SourceNotFound(path.to_string()))?;
            Ok(Some(metadata.len()))
        }
        SourceType::Remote => {
            #[cfg(feature = "remote")]
            {
                // Do a HEAD request to get size
                let client = reqwest::blocking::Client::new();
                let response = client
                    .head(path)
                    .send()
                    .map_err(|e| Error::Network(format!("HEAD request failed: {}", e)))?;

                Ok(response.content_length())
            }
            #[cfg(not(feature = "remote"))]
            {
                Err(Error::InvalidConfig(
                    "Remote support not enabled".to_string(),
                ))
            }
        }
        _ => {
            // Compressed files - return compressed size
            let metadata =
                std::fs::metadata(path).map_err(|_| Error::SourceNotFound(path.to_string()))?;
            Ok(Some(metadata.len()))
        }
    }
}

/// Validate a source path or URL
pub fn validate_source(path: &str) -> Result<SourceInfo> {
    let source_type = detect_source_type(path);

    match source_type {
        SourceType::LocalFile
        | SourceType::Gzip
        | SourceType::Xz
        | SourceType::Zstd
        | SourceType::Bzip2 => {
            let file_path = Path::new(path);
            if !file_path.exists() {
                return Err(Error::SourceNotFound(path.to_string()));
            }

            let metadata = std::fs::metadata(path)?;
            if metadata.is_dir() {
                return Err(Error::InvalidConfig(format!("{} is a directory", path)));
            }

            let size = metadata.len();
            if source_type.is_compressed() {
                Ok(SourceInfo::compressed(path, size, source_type))
            } else {
                Ok(SourceInfo::local(path, size))
            }
        }
        SourceType::Remote => {
            #[cfg(feature = "remote")]
            {
                // Validate URL format
                url::Url::parse(path).map_err(|e| Error::Network(format!("Invalid URL: {}", e)))?;

                // Do a HEAD request to check availability
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .map_err(|e| Error::Network(format!("Failed to create client: {}", e)))?;

                let response = client
                    .head(path)
                    .send()
                    .map_err(|e| Error::Network(format!("Failed to reach URL: {}", e)))?;

                if !response.status().is_success() {
                    return Err(Error::Network(format!(
                        "URL returned status {}",
                        response.status()
                    )));
                }

                let size = response.content_length();
                let resumable = response
                    .headers()
                    .get("accept-ranges")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v == "bytes")
                    .unwrap_or(false);

                Ok(SourceInfo {
                    path: path.to_string(),
                    source_type: SourceType::Remote,
                    compressed_size: size,
                    size,
                    seekable: false,
                    resumable,
                    content_type: response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from),
                    etag: response
                        .headers()
                        .get("etag")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from),
                })
            }
            #[cfg(not(feature = "remote"))]
            {
                Err(Error::InvalidConfig(
                    "Remote support not enabled".to_string(),
                ))
            }
        }
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // -------------------------------------------------------------------------
    // SourceType tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_source_type_is_compressed() {
        assert!(!SourceType::LocalFile.is_compressed());
        assert!(!SourceType::Remote.is_compressed());
        assert!(SourceType::Gzip.is_compressed());
        assert!(SourceType::Xz.is_compressed());
        assert!(SourceType::Zstd.is_compressed());
        assert!(SourceType::Bzip2.is_compressed());
    }

    #[test]
    fn test_source_type_is_remote() {
        assert!(!SourceType::LocalFile.is_remote());
        assert!(SourceType::Remote.is_remote());
        assert!(!SourceType::Gzip.is_remote());
    }

    #[test]
    fn test_source_type_extension() {
        assert_eq!(SourceType::LocalFile.extension(), None);
        assert_eq!(SourceType::Remote.extension(), None);
        assert_eq!(SourceType::Gzip.extension(), Some(".gz"));
        assert_eq!(SourceType::Xz.extension(), Some(".xz"));
        assert_eq!(SourceType::Zstd.extension(), Some(".zst"));
        assert_eq!(SourceType::Bzip2.extension(), Some(".bz2"));
    }

    // -------------------------------------------------------------------------
    // detect_source_type tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_source_type_local() {
        assert_eq!(
            detect_source_type("/path/to/file.iso"),
            SourceType::LocalFile
        );
        assert_eq!(
            detect_source_type("/path/to/file.img"),
            SourceType::LocalFile
        );
        assert_eq!(detect_source_type("file.raw"), SourceType::LocalFile);
        assert_eq!(detect_source_type("file"), SourceType::LocalFile);
    }

    #[test]
    fn test_detect_source_type_remote() {
        assert_eq!(
            detect_source_type("http://example.com/file.iso"),
            SourceType::Remote
        );
        assert_eq!(
            detect_source_type("https://example.com/file.iso"),
            SourceType::Remote
        );
        // Note: compressed remote URLs are detected as Remote, not compression type
        assert_eq!(
            detect_source_type("https://example.com/file.iso.gz"),
            SourceType::Remote
        );
    }

    #[test]
    fn test_detect_source_type_gzip() {
        assert_eq!(detect_source_type("file.iso.gz"), SourceType::Gzip);
        assert_eq!(detect_source_type("file.iso.gzip"), SourceType::Gzip);
        assert_eq!(detect_source_type("FILE.ISO.GZ"), SourceType::Gzip);
    }

    #[test]
    fn test_detect_source_type_xz() {
        assert_eq!(detect_source_type("file.iso.xz"), SourceType::Xz);
        assert_eq!(detect_source_type("FILE.ISO.XZ"), SourceType::Xz);
    }

    #[test]
    fn test_detect_source_type_zstd() {
        assert_eq!(detect_source_type("file.iso.zst"), SourceType::Zstd);
        assert_eq!(detect_source_type("file.iso.zstd"), SourceType::Zstd);
    }

    #[test]
    fn test_detect_source_type_bzip2() {
        assert_eq!(detect_source_type("file.iso.bz2"), SourceType::Bzip2);
        assert_eq!(detect_source_type("file.iso.bzip2"), SourceType::Bzip2);
    }

    // -------------------------------------------------------------------------
    // Magic byte detection tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_compression_from_magic_gzip() {
        let gzip_magic = [0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00];
        assert_eq!(
            detect_compression_from_magic(&gzip_magic),
            Some(SourceType::Gzip)
        );
    }

    #[test]
    fn test_detect_compression_from_magic_xz() {
        let xz_magic = [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00];
        assert_eq!(
            detect_compression_from_magic(&xz_magic),
            Some(SourceType::Xz)
        );
    }

    #[test]
    fn test_detect_compression_from_magic_zstd() {
        let zstd_magic = [0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x00];
        assert_eq!(
            detect_compression_from_magic(&zstd_magic),
            Some(SourceType::Zstd)
        );
    }

    #[test]
    fn test_detect_compression_from_magic_bzip2() {
        let bzip2_magic = [0x42, 0x5a, 0x68, 0x39, 0x00, 0x00]; // BZh9
        assert_eq!(
            detect_compression_from_magic(&bzip2_magic),
            Some(SourceType::Bzip2)
        );
    }

    #[test]
    fn test_detect_compression_from_magic_none() {
        let unknown = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(detect_compression_from_magic(&unknown), None);

        let too_short = [0x1f];
        assert_eq!(detect_compression_from_magic(&too_short), None);
    }

    // -------------------------------------------------------------------------
    // SourceInfo tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_source_info_local() {
        let info = SourceInfo::local("/path/to/file.iso", 1024 * 1024);

        assert_eq!(info.path, "/path/to/file.iso");
        assert_eq!(info.source_type, SourceType::LocalFile);
        assert_eq!(info.size, Some(1024 * 1024));
        assert!(info.seekable);
        assert!(!info.resumable);
    }

    #[test]
    fn test_source_info_compressed() {
        let info = SourceInfo::compressed("/path/to/file.iso.gz", 512 * 1024, SourceType::Gzip);

        assert_eq!(info.path, "/path/to/file.iso.gz");
        assert_eq!(info.source_type, SourceType::Gzip);
        assert_eq!(info.compressed_size, Some(512 * 1024));
        assert_eq!(info.size, None); // Unknown for compressed
        assert!(!info.seekable);
    }

    // -------------------------------------------------------------------------
    // LocalFileSource tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_file_source_open() {
        let mut temp = NamedTempFile::new().unwrap();
        let data = b"Hello, World!";
        temp.write_all(data).unwrap();

        let source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        assert_eq!(source.info().size, Some(data.len() as u64));
        assert!(source.info().seekable);
    }

    #[test]
    fn test_local_file_source_read() {
        let mut temp = NamedTempFile::new().unwrap();
        let data = b"Test data for reading";
        temp.write_all(data).unwrap();

        let mut source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        let mut buffer = vec![0u8; data.len()];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, data);
    }

    #[test]
    fn test_local_file_source_seek() {
        let mut temp = NamedTempFile::new().unwrap();
        let data = b"0123456789";
        temp.write_all(data).unwrap();

        let mut source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        // Seek to middle
        source.seek(SeekFrom::Start(5)).unwrap();

        let mut buffer = vec![0u8; 5];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"56789");
    }

    #[test]
    fn test_local_file_source_not_found() {
        let result = LocalFileSource::open("/nonexistent/path/to/file.iso");
        assert!(matches!(result, Err(Error::SourceNotFound(_))));
    }

    // -------------------------------------------------------------------------
    // Source unified interface tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_source_open_local() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test data").unwrap();

        let source = Source::open(temp.path().to_str().unwrap()).unwrap();

        assert!(!source.is_compressed());
        assert!(source.is_seekable());
        assert_eq!(source.size(), Some(9));
    }

    #[test]
    fn test_source_read() {
        let mut temp = NamedTempFile::new().unwrap();
        let data = b"Source read test";
        temp.write_all(data).unwrap();

        let mut source = Source::open(temp.path().to_str().unwrap()).unwrap();

        let mut buffer = vec![0u8; data.len()];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, data);
    }

    #[test]
    fn test_source_not_found() {
        let result = Source::open("/nonexistent/file.iso");
        assert!(matches!(result, Err(Error::SourceNotFound(_))));
    }

    // -------------------------------------------------------------------------
    // Compression tests (require compression feature)
    // -------------------------------------------------------------------------

    #[cfg(feature = "compression")]
    #[test]
    fn test_source_open_gzip() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        // Create a gzip file
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".gz";

        let file = File::create(&path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(b"Hello from gzip!").unwrap();
        encoder.finish().unwrap();

        // Open and read
        let mut source = Source::open(&path).unwrap();
        assert!(source.is_compressed());

        let mut buffer = String::new();
        source.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "Hello from gzip!");

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(feature = "compression")]
    #[test]
    fn test_source_open_xz() {
        use xz2::write::XzEncoder;

        // Create an xz file
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".xz";

        let file = File::create(&path).unwrap();
        let mut encoder = XzEncoder::new(file, 6);
        encoder.write_all(b"Hello from xz!").unwrap();
        encoder.finish().unwrap();

        // Open and read
        let mut source = Source::open(&path).unwrap();
        assert!(source.is_compressed());

        let mut buffer = String::new();
        source.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "Hello from xz!");

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(feature = "compression")]
    #[test]
    fn test_source_open_zstd() {
        // Create a zstd file
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".zst";

        let file = File::create(&path).unwrap();
        let mut encoder = zstd::Encoder::new(file, 3).unwrap();
        encoder.write_all(b"Hello from zstd!").unwrap();
        encoder.finish().unwrap();

        // Open and read
        let mut source = Source::open(&path).unwrap();
        assert!(source.is_compressed());

        let mut buffer = String::new();
        source.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "Hello from zstd!");

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(feature = "compression")]
    #[test]
    fn test_source_open_bzip2() {
        use bzip2::write::BzEncoder;
        use bzip2::Compression;

        // Create a bzip2 file
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".bz2";

        let file = File::create(&path).unwrap();
        let mut encoder = BzEncoder::new(file, Compression::default());
        encoder.write_all(b"Hello from bzip2!").unwrap();
        encoder.finish().unwrap();

        // Open and read
        let mut source = Source::open(&path).unwrap();
        assert!(source.is_compressed());

        let mut buffer = String::new();
        source.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "Hello from bzip2!");

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }

    // -------------------------------------------------------------------------
    // get_source_size tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_source_size_local() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 1024]).unwrap();

        let size = get_source_size(temp.path().to_str().unwrap()).unwrap();
        assert_eq!(size, Some(1024));
    }

    #[test]
    fn test_get_source_size_not_found() {
        let result = get_source_size("/nonexistent/file.iso");
        assert!(matches!(result, Err(Error::SourceNotFound(_))));
    }

    // -------------------------------------------------------------------------
    // validate_source tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_source_local() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 2048]).unwrap();

        let info = validate_source(temp.path().to_str().unwrap()).unwrap();

        assert_eq!(info.size, Some(2048));
        assert_eq!(info.source_type, SourceType::LocalFile);
    }

    #[test]
    fn test_validate_source_not_found() {
        let result = validate_source("/nonexistent/file.iso");
        assert!(matches!(result, Err(Error::SourceNotFound(_))));
    }

    #[test]
    fn test_validate_source_directory_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = validate_source(temp_dir.path().to_str().unwrap());
        assert!(matches!(result, Err(Error::InvalidConfig(_))));
    }

    #[test]
    fn test_validate_source_compressed() {
        // Create a temp file with .gz extension
        let temp_dir = tempfile::tempdir().unwrap();
        let gz_path = temp_dir.path().join("test.iso.gz");
        std::fs::write(&gz_path, [0u8; 1024]).unwrap();

        let info = validate_source(gz_path.to_str().unwrap()).unwrap();
        assert_eq!(info.source_type, SourceType::Gzip);
        assert_eq!(info.compressed_size, Some(1024));
        assert!(!info.seekable);
    }

    // -------------------------------------------------------------------------
    // Source with offset tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_source_open_with_offset() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"0123456789ABCDEF").unwrap();

        let mut source = Source::open_with_offset(temp.path().to_str().unwrap(), 10).unwrap();

        let mut buffer = vec![0u8; 6];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"ABCDEF");
    }

    #[cfg(feature = "compression")]
    #[test]
    fn test_source_compressed_cannot_resume() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.gz");

        // Create compressed file
        let file = File::create(&path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(b"test data").unwrap();
        encoder.finish().unwrap();

        // Try to open with offset - should fail
        let result = Source::open_with_offset(path.to_str().unwrap(), 100);
        assert!(matches!(result, Err(Error::InvalidConfig(_))));
    }

    // -------------------------------------------------------------------------
    // Source helper method tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_source_size() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();

        let source = Source::open(temp.path().to_str().unwrap()).unwrap();
        assert_eq!(source.size(), Some(4096));
    }

    #[test]
    fn test_source_is_seekable() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test").unwrap();

        let source = Source::open(temp.path().to_str().unwrap()).unwrap();
        assert!(source.is_seekable());
    }

    #[test]
    fn test_source_is_compressed() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test").unwrap();

        let source = Source::open(temp.path().to_str().unwrap()).unwrap();
        assert!(!source.is_compressed());
    }

    // -------------------------------------------------------------------------
    // SourceInfo additional tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_source_info_local_with_compression_extension() {
        // SourceInfo::local uses detect_source_type
        let info = SourceInfo::local("/path/to/file.iso.gz", 1024);
        assert_eq!(info.source_type, SourceType::Gzip);
    }

    #[test]
    fn test_source_info_compressed_properties() {
        let info = SourceInfo::compressed("/path/to/file.iso.xz", 512, SourceType::Xz);

        assert_eq!(info.path, "/path/to/file.iso.xz");
        assert_eq!(info.source_type, SourceType::Xz);
        assert_eq!(info.compressed_size, Some(512));
        assert_eq!(info.size, None);
        assert!(!info.seekable);
        assert!(!info.resumable);
        assert!(info.content_type.is_none());
        assert!(info.etag.is_none());
    }

    // -------------------------------------------------------------------------
    // open_file_buffered tests (via Source::open)
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_file_buffered_permission_denied() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let temp = NamedTempFile::new().unwrap();
            let path = temp.path();

            // Remove read permission
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(path, perms).unwrap();

            let result = Source::open(path.to_str().unwrap());

            // Restore permissions for cleanup
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(path, perms).unwrap();

            assert!(matches!(result, Err(Error::PermissionDenied(_))));
        }
    }

    // -------------------------------------------------------------------------
    // get_source_size additional tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_source_size_compressed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.iso.xz");
        std::fs::write(&path, [0u8; 2048]).unwrap();

        // For compressed files, returns compressed size
        let size = get_source_size(path.to_str().unwrap()).unwrap();
        assert_eq!(size, Some(2048));
    }

    // -------------------------------------------------------------------------
    // LocalFileSource additional tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_local_file_source_large_read() {
        let mut temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
        temp.write_all(&data).unwrap();

        let mut source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        let mut buffer = vec![0u8; 65536];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(buffer, data);
    }

    #[test]
    fn test_local_file_source_partial_read() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"Hello, World!").unwrap();

        let mut source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        // Read first 5 bytes
        let mut buffer = vec![0u8; 5];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"Hello");

        // Read next 8 bytes (including EOF)
        let mut buffer = vec![0u8; 20];
        let n = source.read(&mut buffer).unwrap();
        assert_eq!(n, 8); // ", World!"
        assert_eq!(&buffer[..n], b", World!");
    }

    #[test]
    fn test_local_file_source_seek_from_end() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"0123456789").unwrap();

        let mut source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        // Seek to 3 bytes before end
        source.seek(SeekFrom::End(-3)).unwrap();

        let mut buffer = vec![0u8; 3];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"789");
    }

    #[test]
    fn test_local_file_source_seek_from_current() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"ABCDEFGHIJ").unwrap();

        let mut source = LocalFileSource::open(temp.path().to_str().unwrap()).unwrap();

        // Read 2 bytes
        let mut buffer = vec![0u8; 2];
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"AB");

        // Skip 3 more bytes
        source.seek(SeekFrom::Current(3)).unwrap();

        // Read from position 5
        source.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"FG");
    }
}
