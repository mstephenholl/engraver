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
use crate::settings::{NetworkSettings, WriteSettings};
#[cfg(feature = "remote")]
use crate::settings::{DEFAULT_HTTP_TIMEOUT_SECS, DEFAULT_VALIDATION_TIMEOUT_SECS};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
use std::sync::Arc;

/// Default read buffer size in bytes (64 KB)
pub const DEFAULT_READ_BUFFER_SIZE: usize = 64 * 1024;

/// Default cloud chunk size in bytes (4 MB)
#[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
pub const DEFAULT_CLOUD_CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// Parse a size string (e.g., "64K", "4M") to bytes
///
/// Returns the default value if parsing fails.
fn parse_size_with_default(s: &str, default: usize) -> usize {
    crate::benchmark::parse_size(s)
        .map(|v| v as usize)
        .unwrap_or(default)
}

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
    /// AWS S3 or S3-compatible storage (s3://)
    #[cfg(feature = "s3")]
    S3,
    /// Google Cloud Storage (gs://)
    #[cfg(feature = "gcs")]
    Gcs,
    /// Azure Blob Storage (azure://)
    #[cfg(feature = "azure")]
    Azure,
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

    /// Check if this source type is cloud storage (S3, GCS, or Azure)
    pub fn is_cloud(&self) -> bool {
        #[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
        {
            match self {
                #[cfg(feature = "s3")]
                SourceType::S3 => true,
                #[cfg(feature = "gcs")]
                SourceType::Gcs => true,
                #[cfg(feature = "azure")]
                SourceType::Azure => true,
                _ => false,
            }
        }
        #[cfg(not(any(feature = "s3", feature = "gcs", feature = "azure")))]
        {
            false
        }
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
    // Check for cloud URIs first
    #[cfg(feature = "s3")]
    if path.starts_with("s3://") {
        return SourceType::S3;
    }

    #[cfg(feature = "gcs")]
    if path.starts_with("gs://") {
        return SourceType::Gcs;
    }

    #[cfg(feature = "azure")]
    if path.starts_with("azure://") {
        return SourceType::Azure;
    }

    // Check for remote URLs
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
    /// Open a local file with default buffer size
    pub fn open(path: &str) -> Result<Self> {
        Self::open_with_settings(path, None)
    }

    /// Open a local file with custom settings
    ///
    /// If `settings` is `None`, default buffer size is used.
    pub fn open_with_settings(path: &str, settings: Option<&WriteSettings>) -> Result<Self> {
        let buffer_size = settings
            .map(|s| parse_size_with_default(&s.read_buffer_size, DEFAULT_READ_BUFFER_SIZE))
            .unwrap_or(DEFAULT_READ_BUFFER_SIZE);

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
            file: BufReader::with_capacity(buffer_size, file),
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
        Self::open_with_settings(url, 0, None)
    }

    /// Open an HTTP/HTTPS URL with resume from a specific offset
    pub fn open_with_resume(url: &str, offset: u64) -> Result<Self> {
        Self::open_with_settings(url, offset, None)
    }

    /// Open an HTTP/HTTPS URL with custom network settings
    ///
    /// If `settings` is `None`, default timeout values are used.
    pub fn open_with_settings(
        url: &str,
        offset: u64,
        settings: Option<&NetworkSettings>,
    ) -> Result<Self> {
        let timeout_secs = settings
            .map(|s| s.http_timeout_secs)
            .unwrap_or(DEFAULT_HTTP_TIMEOUT_SECS);

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
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| Error::Network(format!("Failed to create HTTP client: {}", e)))?;

        let mut request = client.get(url);

        // Add Range header for resume
        if offset > 0 {
            request = request.header("Range", format!("bytes={}-", offset));
        }

        // Send request
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                Error::Network(format!(
                    "HTTP request timed out after {} seconds: {}",
                    timeout_secs, e
                ))
            } else if e.is_connect() {
                Error::Network(format!("Failed to connect to server: {}", e))
            } else {
                Error::Network(format!("HTTP request failed: {}", e))
            }
        })?;

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
// Cloud Storage Source (S3/GCS/Azure)
// ============================================================================

/// Cloud storage source using object_store crate
///
/// Supports AWS S3, S3-compatible services (MinIO, DigitalOcean Spaces, etc.),
/// Google Cloud Storage, and Azure Blob Storage.
///
/// ## S3-Compatible Services
///
/// For S3-compatible services, set the `AWS_ENDPOINT_URL` environment variable:
/// - DigitalOcean Spaces: `https://nyc3.digitaloceanspaces.com`
/// - MinIO: `http://localhost:9000`
/// - Backblaze B2: `https://s3.us-west-000.backblazeb2.com`
///
/// ## Authentication
///
/// Credentials are automatically discovered from:
/// - Environment variables (AWS_ACCESS_KEY_ID, GOOGLE_APPLICATION_CREDENTIALS, etc.)
/// - Config files (~/.aws/credentials, service account JSON)
/// - Instance metadata (IAM roles, managed identities)
#[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
pub struct CloudSource {
    /// Buffered chunks from the cloud object
    buffer: Vec<u8>,
    /// Current position in buffer
    buffer_pos: usize,
    /// Runtime for async operations
    runtime: tokio::runtime::Runtime,
    /// The object store client
    store: Arc<dyn object_store::ObjectStore>,
    /// Object location/path within the store
    location: object_store::path::Path,
    /// Current read offset in the object
    offset: u64,
    /// Total object size
    total_size: u64,
    /// Source info
    info: SourceInfo,
    /// Bytes read so far (including resume offset)
    bytes_read: u64,
    /// Chunk size for streaming reads
    chunk_size: u64,
}

#[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
impl CloudSource {
    /// Open a cloud object from URI with default settings
    pub fn open(uri: &str) -> Result<Self> {
        Self::open_with_settings(uri, 0, None)
    }

    /// Open with resume from specific offset
    ///
    /// This uses Range headers to resume from a specific byte offset,
    /// which is useful for resuming interrupted writes.
    pub fn open_with_resume(uri: &str, offset: u64) -> Result<Self> {
        Self::open_with_settings(uri, offset, None)
    }

    /// Open with resume and custom network settings
    ///
    /// If `settings` is `None`, default chunk size is used.
    pub fn open_with_settings(
        uri: &str,
        offset: u64,
        settings: Option<&NetworkSettings>,
    ) -> Result<Self> {
        let chunk_size = settings
            .map(|s| {
                crate::benchmark::parse_size(&s.cloud_chunk_size)
                    .unwrap_or(DEFAULT_CLOUD_CHUNK_SIZE)
            })
            .unwrap_or(DEFAULT_CLOUD_CHUNK_SIZE);

        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| Error::Network(format!("Failed to create tokio runtime: {}", e)))?;

        let (store, location, source_type) =
            runtime.block_on(async { Self::create_store(uri).await })?;

        // Get object metadata
        let meta = runtime
            .block_on(async { store.head(&location).await })
            .map_err(|e| Error::Network(format!("Failed to get object metadata: {}", e)))?;

        let total_size = meta.size as u64;
        let etag = meta.e_tag.clone();

        // Validate offset
        if offset > total_size {
            return Err(Error::InvalidConfig(format!(
                "Resume offset {} exceeds object size {}",
                offset, total_size
            )));
        }

        let info = SourceInfo {
            path: uri.to_string(),
            source_type,
            compressed_size: Some(total_size),
            size: Some(total_size),
            seekable: false,
            resumable: true, // Cloud storage supports Range headers
            content_type: None,
            etag,
        };

        Ok(Self {
            buffer: Vec::new(),
            buffer_pos: 0,
            runtime,
            store,
            location,
            offset,
            total_size,
            info,
            bytes_read: offset,
            chunk_size,
        })
    }

    /// Create the appropriate object store client based on URI scheme
    async fn create_store(
        uri: &str,
    ) -> Result<(
        Arc<dyn object_store::ObjectStore>,
        object_store::path::Path,
        SourceType,
    )> {
        #[cfg(feature = "s3")]
        if uri.starts_with("s3://") {
            let (bucket, key) = parse_s3_uri(uri)?;
            let store = object_store::aws::AmazonS3Builder::from_env()
                .with_bucket_name(&bucket)
                .build()
                .map_err(|e| Error::Network(format!("Failed to create S3 client: {}", e)))?;
            return Ok((
                Arc::new(store),
                object_store::path::Path::from(key),
                SourceType::S3,
            ));
        }

        #[cfg(feature = "gcs")]
        if uri.starts_with("gs://") {
            let (bucket, object) = parse_gcs_uri(uri)?;
            let store = object_store::gcp::GoogleCloudStorageBuilder::from_env()
                .with_bucket_name(&bucket)
                .build()
                .map_err(|e| Error::Network(format!("Failed to create GCS client: {}", e)))?;
            return Ok((
                Arc::new(store),
                object_store::path::Path::from(object),
                SourceType::Gcs,
            ));
        }

        #[cfg(feature = "azure")]
        if uri.starts_with("azure://") {
            let (account, container, blob) = parse_azure_uri(uri)?;
            let store = object_store::azure::MicrosoftAzureBuilder::from_env()
                .with_account(&account)
                .with_container_name(&container)
                .build()
                .map_err(|e| Error::Network(format!("Failed to create Azure client: {}", e)))?;
            return Ok((
                Arc::new(store),
                object_store::path::Path::from(blob),
                SourceType::Azure,
            ));
        }

        Err(Error::InvalidConfig(format!(
            "Unsupported cloud URI scheme: {}",
            uri
        )))
    }

    /// Fill the internal buffer with the next chunk from cloud storage
    fn fill_buffer(&mut self) -> std::io::Result<()> {
        if self.offset >= self.total_size {
            return Ok(());
        }

        let end = std::cmp::min(self.offset + self.chunk_size, self.total_size);
        let range = std::ops::Range {
            start: self.offset as usize,
            end: end as usize,
        };

        let result = self
            .runtime
            .block_on(async { self.store.get_range(&self.location, range).await })
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        self.buffer = result.to_vec();
        self.buffer_pos = 0;
        self.offset = end;
        Ok(())
    }

    /// Get source info
    pub fn info(&self) -> &SourceInfo {
        &self.info
    }

    /// Get bytes read so far (including resume offset)
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Check if this source supports resume
    pub fn supports_resume(&self) -> bool {
        true
    }
}

#[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
impl Read for CloudSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Refill buffer if empty
        if self.buffer_pos >= self.buffer.len() {
            self.fill_buffer()?;
            if self.buffer.is_empty() {
                return Ok(0); // EOF
            }
        }

        let available = self.buffer.len() - self.buffer_pos;
        let to_read = std::cmp::min(buf.len(), available);
        buf[..to_read].copy_from_slice(&self.buffer[self.buffer_pos..self.buffer_pos + to_read]);
        self.buffer_pos += to_read;
        self.bytes_read += to_read as u64;
        Ok(to_read)
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

    /// Cloud storage source (S3, GCS, Azure)
    #[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
    Cloud(CloudSource),

    /// Cloud source with gzip compression
    #[cfg(all(
        any(feature = "s3", feature = "gcs", feature = "azure"),
        feature = "compression"
    ))]
    CloudGzip(GzipSource<CloudSource>),

    /// Cloud source with xz compression
    #[cfg(all(
        any(feature = "s3", feature = "gcs", feature = "azure"),
        feature = "compression"
    ))]
    CloudXz(XzSource<CloudSource>),

    /// Cloud source with zstd compression
    #[cfg(all(
        any(feature = "s3", feature = "gcs", feature = "azure"),
        feature = "compression"
    ))]
    CloudZstd(Box<ZstdSource<'static, CloudSource>>),

    /// Cloud source with bzip2 compression
    #[cfg(all(
        any(feature = "s3", feature = "gcs", feature = "azure"),
        feature = "compression"
    ))]
    CloudBzip2(Bzip2Source<CloudSource>),
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

            #[cfg(feature = "s3")]
            SourceType::S3 => {
                let cloud_source = CloudSource::open_with_resume(path, offset)?;
                Ok(Source::Cloud(cloud_source))
            }

            #[cfg(feature = "gcs")]
            SourceType::Gcs => {
                let cloud_source = CloudSource::open_with_resume(path, offset)?;
                Ok(Source::Cloud(cloud_source))
            }

            #[cfg(feature = "azure")]
            SourceType::Azure => {
                let cloud_source = CloudSource::open_with_resume(path, offset)?;
                Ok(Source::Cloud(cloud_source))
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
            #[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
            Source::Cloud(s) => s.info(),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudGzip(s) => s.info(),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudXz(s) => s.info(),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudZstd(s) => s.info(),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudBzip2(s) => s.info(),
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
            #[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
            Source::Cloud(s) => s.read(buf),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudGzip(s) => s.read(buf),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudXz(s) => s.read(buf),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudZstd(s) => s.read(buf),
            #[cfg(all(
                any(feature = "s3", feature = "gcs", feature = "azure"),
                feature = "compression"
            ))]
            Source::CloudBzip2(s) => s.read(buf),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse S3 URI (s3://bucket/key) into (bucket, key)
#[cfg(feature = "s3")]
fn parse_s3_uri(uri: &str) -> Result<(String, String)> {
    let without_scheme = uri
        .strip_prefix("s3://")
        .ok_or_else(|| Error::InvalidConfig("Invalid S3 URI format".to_string()))?;
    let parts: Vec<&str> = without_scheme.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(Error::InvalidConfig(
            "S3 URI must be s3://bucket/key".to_string(),
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse GCS URI (gs://bucket/object) into (bucket, object)
#[cfg(feature = "gcs")]
fn parse_gcs_uri(uri: &str) -> Result<(String, String)> {
    let without_scheme = uri
        .strip_prefix("gs://")
        .ok_or_else(|| Error::InvalidConfig("Invalid GCS URI format".to_string()))?;
    let parts: Vec<&str> = without_scheme.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(Error::InvalidConfig(
            "GCS URI must be gs://bucket/object".to_string(),
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse Azure URI (azure://account/container/blob) into (account, container, blob)
#[cfg(feature = "azure")]
fn parse_azure_uri(uri: &str) -> Result<(String, String, String)> {
    let without_scheme = uri
        .strip_prefix("azure://")
        .ok_or_else(|| Error::InvalidConfig("Invalid Azure URI format".to_string()))?;
    let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(Error::InvalidConfig(
            "Azure URI must be azure://account/container/blob".to_string(),
        ));
    }
    Ok((
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
    ))
}

/// Open a file with buffered reading
fn open_file_buffered(path: &str) -> Result<BufReader<File>> {
    open_file_buffered_with_size(path, DEFAULT_READ_BUFFER_SIZE)
}

/// Open a file with buffered reading and custom buffer size
fn open_file_buffered_with_size(path: &str, buffer_size: usize) -> Result<BufReader<File>> {
    let file = File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::SourceNotFound(path.to_string())
        } else if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::PermissionDenied(format!("Cannot read {}: {}", path, e))
        } else {
            Error::Io(e)
        }
    })?;

    Ok(BufReader::with_capacity(buffer_size, file))
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
///
/// Uses default network settings. For custom timeouts, use [`validate_source_with_settings`].
pub fn validate_source(path: &str) -> Result<SourceInfo> {
    validate_source_with_settings(path, None)
}

/// Validate a source path or URL with custom network settings
///
/// If `settings` is `None`, default timeout values are used.
#[allow(unused_variables)] // settings only used with remote feature
pub fn validate_source_with_settings(
    path: &str,
    settings: Option<&NetworkSettings>,
) -> Result<SourceInfo> {
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
                let timeout_secs = settings
                    .map(|s| s.validation_timeout_secs)
                    .unwrap_or(DEFAULT_VALIDATION_TIMEOUT_SECS);

                // Validate URL format
                url::Url::parse(path).map_err(|e| Error::Network(format!("Invalid URL: {}", e)))?;

                // Do a HEAD request to check availability
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(timeout_secs))
                    .build()
                    .map_err(|e| Error::Network(format!("Failed to create client: {}", e)))?;

                let response = client.head(path).send().map_err(|e| {
                    if e.is_timeout() {
                        Error::Network(format!(
                            "URL validation timed out after {} seconds: {}",
                            timeout_secs, e
                        ))
                    } else if e.is_connect() {
                        Error::Network(format!("Failed to connect to URL: {}", e))
                    } else {
                        Error::Network(format!("Failed to reach URL: {}", e))
                    }
                })?;

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
        #[cfg(feature = "s3")]
        SourceType::S3 => {
            // Validate S3 URI and check object exists via HEAD request
            validate_cloud_source(path, source_type)
        }
        #[cfg(feature = "gcs")]
        SourceType::Gcs => {
            // Validate GCS URI and check object exists via HEAD request
            validate_cloud_source(path, source_type)
        }
        #[cfg(feature = "azure")]
        SourceType::Azure => {
            // Validate Azure URI and check object exists via HEAD request
            validate_cloud_source(path, source_type)
        }
    }
}

/// Validate a cloud source by checking if the object exists (via HEAD/metadata request)
#[cfg(any(feature = "s3", feature = "gcs", feature = "azure"))]
fn validate_cloud_source(path: &str, source_type: SourceType) -> Result<SourceInfo> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| Error::Network(format!("Failed to create tokio runtime: {}", e)))?;

    let (store, location, _) = runtime.block_on(async { CloudSource::create_store(path).await })?;

    // Use HEAD request (object metadata) to check existence without downloading
    let meta = runtime
        .block_on(async { store.head(&location).await })
        .map_err(|e| Error::Network(format!("Object not found or inaccessible: {}", e)))?;

    Ok(SourceInfo {
        path: path.to_string(),
        source_type,
        compressed_size: Some(meta.size as u64),
        size: Some(meta.size as u64),
        seekable: false,
        resumable: true,
        content_type: None,
        etag: meta.e_tag,
    })
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

    #[test]
    fn test_source_type_is_cloud() {
        // Non-cloud types should return false
        assert!(!SourceType::LocalFile.is_cloud());
        assert!(!SourceType::Remote.is_cloud());
        assert!(!SourceType::Gzip.is_cloud());
        assert!(!SourceType::Xz.is_cloud());
        assert!(!SourceType::Zstd.is_cloud());
        assert!(!SourceType::Bzip2.is_cloud());

        // Cloud types should return true (when features are enabled)
        #[cfg(feature = "s3")]
        assert!(SourceType::S3.is_cloud());
        #[cfg(feature = "gcs")]
        assert!(SourceType::Gcs.is_cloud());
        #[cfg(feature = "azure")]
        assert!(SourceType::Azure.is_cloud());
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

    #[cfg(feature = "s3")]
    #[test]
    fn test_detect_source_type_s3() {
        assert_eq!(detect_source_type("s3://bucket/key"), SourceType::S3);
        assert_eq!(
            detect_source_type("s3://my-bucket/path/to/image.iso"),
            SourceType::S3
        );
        // Even if it looks like compressed, s3:// scheme takes precedence
        assert_eq!(
            detect_source_type("s3://bucket/image.iso.gz"),
            SourceType::S3
        );
    }

    #[cfg(feature = "gcs")]
    #[test]
    fn test_detect_source_type_gcs() {
        assert_eq!(detect_source_type("gs://bucket/object"), SourceType::Gcs);
        assert_eq!(
            detect_source_type("gs://my-bucket/path/to/image.iso"),
            SourceType::Gcs
        );
    }

    #[cfg(feature = "azure")]
    #[test]
    fn test_detect_source_type_azure() {
        assert_eq!(
            detect_source_type("azure://account/container/blob"),
            SourceType::Azure
        );
        assert_eq!(
            detect_source_type("azure://storageaccount/images/ubuntu.iso"),
            SourceType::Azure
        );
    }

    // -------------------------------------------------------------------------
    // Cloud URI parsing tests
    // -------------------------------------------------------------------------

    #[cfg(feature = "s3")]
    #[test]
    fn test_parse_s3_uri_valid() {
        let (bucket, key) = parse_s3_uri("s3://my-bucket/path/to/file.iso").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "path/to/file.iso");
    }

    #[cfg(feature = "s3")]
    #[test]
    fn test_parse_s3_uri_simple() {
        let (bucket, key) = parse_s3_uri("s3://bucket/key").unwrap();
        assert_eq!(bucket, "bucket");
        assert_eq!(key, "key");
    }

    #[cfg(feature = "s3")]
    #[test]
    fn test_parse_s3_uri_invalid() {
        // Missing key
        assert!(parse_s3_uri("s3://bucket/").is_err());
        assert!(parse_s3_uri("s3://bucket").is_err());
        // Missing bucket
        assert!(parse_s3_uri("s3:///key").is_err());
        // Wrong scheme
        assert!(parse_s3_uri("gs://bucket/key").is_err());
    }

    #[cfg(feature = "gcs")]
    #[test]
    fn test_parse_gcs_uri_valid() {
        let (bucket, object) = parse_gcs_uri("gs://my-bucket/path/to/file.iso").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(object, "path/to/file.iso");
    }

    #[cfg(feature = "gcs")]
    #[test]
    fn test_parse_gcs_uri_invalid() {
        assert!(parse_gcs_uri("gs://bucket/").is_err());
        assert!(parse_gcs_uri("gs://bucket").is_err());
        assert!(parse_gcs_uri("s3://bucket/key").is_err());
    }

    #[cfg(feature = "azure")]
    #[test]
    fn test_parse_azure_uri_valid() {
        let (account, container, blob) =
            parse_azure_uri("azure://storageaccount/mycontainer/path/to/blob.iso").unwrap();
        assert_eq!(account, "storageaccount");
        assert_eq!(container, "mycontainer");
        assert_eq!(blob, "path/to/blob.iso");
    }

    #[cfg(feature = "azure")]
    #[test]
    fn test_parse_azure_uri_invalid() {
        // Missing blob
        assert!(parse_azure_uri("azure://account/container/").is_err());
        // Missing container
        assert!(parse_azure_uri("azure://account//blob").is_err());
        // Missing account
        assert!(parse_azure_uri("azure:///container/blob").is_err());
        // Too few parts
        assert!(parse_azure_uri("azure://account/container").is_err());
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
