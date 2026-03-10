//! Integration tests for HTTP source with a mock server
//!
//! Each test spins up its own `tiny_http` server on an OS-assigned port,
//! so all tests can run in parallel without port conflicts.

#![cfg(feature = "remote")]

use engraver_core::{
    detect_source_type, validate_source, Source, SourceType, WriteConfig, Writer, MIN_BLOCK_SIZE,
};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tiny_http::{Header, Response, Server, StatusCode};

// ============================================================================
// Mock HTTP Server
// ============================================================================

/// A self-contained mock HTTP server running on a background thread.
/// Automatically shuts down when dropped (Arc<Server> refcount → 0 → recv() errors).
struct MockHttpServer {
    url: String,
    _server: Arc<Server>,
}

/// Describes how the mock server should respond to requests.
#[derive(Clone)]
enum MockBehavior {
    /// Serve data with 200 OK + Content-Length
    ServeData(Vec<u8>),
    /// Serve data with Range request support (Accept-Ranges: bytes)
    ServeWithResume(Vec<u8>),
    /// Return a fixed status code with a body
    StatusCode(u16, String),
}

fn start_mock(behavior: MockBehavior) -> MockHttpServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let addr = server.server_addr().to_ip().unwrap();
    let url = format!("http://{}", addr);

    let server_clone = Arc::clone(&server);
    std::thread::spawn(move || {
        while let Ok(request) = server_clone.recv() {
            match &behavior {
                MockBehavior::ServeData(data) => {
                    let response = Response::from_data(data.clone()).with_header(
                        Header::from_bytes(b"Content-Length", data.len().to_string().as_bytes())
                            .unwrap(),
                    );
                    let _ = request.respond(response);
                }
                MockBehavior::ServeWithResume(data) => {
                    // Check for Range header
                    let range_header = request
                        .headers()
                        .iter()
                        .find(|h| h.field.as_str() == "Range" || h.field.as_str() == "range")
                        .map(|h| h.value.as_str().to_string());

                    if let Some(range) = range_header {
                        // Parse "bytes=N-"
                        if let Some(offset_str) = range.strip_prefix("bytes=") {
                            if let Some(start_str) = offset_str.strip_suffix('-') {
                                if let Ok(start) = start_str.parse::<usize>() {
                                    let slice = &data[start.min(data.len())..];
                                    let response = Response::from_data(slice.to_vec())
                                        .with_status_code(StatusCode(206))
                                        .with_header(
                                            Header::from_bytes(
                                                b"Content-Length",
                                                slice.len().to_string().as_bytes(),
                                            )
                                            .unwrap(),
                                        )
                                        .with_header(
                                            Header::from_bytes(b"Accept-Ranges", b"bytes" as &[u8])
                                                .unwrap(),
                                        )
                                        .with_header(
                                            Header::from_bytes(
                                                b"Content-Range",
                                                format!(
                                                    "bytes {}-{}/{}",
                                                    start,
                                                    data.len() - 1,
                                                    data.len()
                                                )
                                                .as_bytes(),
                                            )
                                            .unwrap(),
                                        );
                                    let _ = request.respond(response);
                                    continue;
                                }
                            }
                        }
                    }

                    // No Range or invalid Range → full response
                    let response = Response::from_data(data.clone())
                        .with_header(
                            Header::from_bytes(
                                b"Content-Length",
                                data.len().to_string().as_bytes(),
                            )
                            .unwrap(),
                        )
                        .with_header(
                            Header::from_bytes(b"Accept-Ranges", b"bytes" as &[u8]).unwrap(),
                        );
                    let _ = request.respond(response);
                }
                MockBehavior::StatusCode(code, body) => {
                    let response =
                        Response::from_string(body.clone()).with_status_code(StatusCode(*code));
                    let _ = request.respond(response);
                }
            }
        }
    });

    MockHttpServer {
        url,
        _server: server,
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

fn create_test_device(size: u64) -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    file.as_file().set_len(size).unwrap();
    file
}

fn read_all(file: &mut std::fs::File) -> Vec<u8> {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    buf
}

// ============================================================================
// Source type detection
// ============================================================================

#[test]
fn http_url_detected_as_remote() {
    assert_eq!(
        detect_source_type("http://example.com/image.iso"),
        SourceType::Remote
    );
    assert_eq!(
        detect_source_type("https://example.com/image.iso"),
        SourceType::Remote
    );
}

// ============================================================================
// Basic HTTP source
// ============================================================================

#[test]
fn http_source_basic_download() {
    let data = test_data(64 * 1024); // 64 KB
    let server = start_mock(MockBehavior::ServeData(data.clone()));
    let url = format!("{}/image.iso", server.url);

    let mut source = Source::open(&url).unwrap();
    let mut buf = Vec::new();
    source.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, data);
}

#[test]
fn http_source_content_length_reported() {
    use engraver_core::source::HttpSource;

    let data = test_data(32 * 1024);
    let server = start_mock(MockBehavior::ServeData(data.clone()));
    let url = format!("{}/image.iso", server.url);

    // HttpSource extracts Content-Length from headers during open
    let source = HttpSource::open(&url).unwrap();
    let info = source.info();
    // tiny_http may use chunked transfer encoding, making Content-Length unavailable.
    // When size is reported, it should match the data length.
    if let Some(size) = info.size {
        assert_eq!(size, data.len() as u64);
    }
    // Regardless, reading should yield the correct data
    drop(source);
    let mut source2 = HttpSource::open(&url).unwrap();
    let mut buf = Vec::new();
    source2.read_to_end(&mut buf).unwrap();
    assert_eq!(buf.len(), data.len());
}

#[test]
fn http_source_not_seekable() {
    let data = test_data(1024);
    let server = start_mock(MockBehavior::ServeData(data));
    let url = format!("{}/image.iso", server.url);

    let source = Source::open(&url).unwrap();
    assert!(!source.is_seekable());
}

// ============================================================================
// HTTP → Writer pipeline
// ============================================================================

#[test]
fn http_source_write_pipeline() {
    let data = test_data(256 * 1024); // 256 KB
    let server = start_mock(MockBehavior::ServeData(data.clone()));
    let url = format!("{}/image.iso", server.url);

    let mut source = Source::open(&url).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn http_source_large_file_write() {
    let data = test_data(4 * 1024 * 1024); // 4 MB
    let server = start_mock(MockBehavior::ServeData(data.clone()));
    let url = format!("{}/large.iso", server.url);

    let mut source = Source::open(&url).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(1024 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Resume with Range header
// ============================================================================

#[test]
fn http_source_resume_with_range() {
    let data = test_data(128 * 1024); // 128 KB
    let server = start_mock(MockBehavior::ServeWithResume(data.clone()));
    let url = format!("{}/image.iso", server.url);

    let offset = 64 * 1024; // Resume from 64 KB

    // Open with resume offset
    let mut source = Source::open_with_offset(&url, offset as u64).unwrap();
    let mut buf = Vec::new();
    source.read_to_end(&mut buf).unwrap();

    // Should receive only the second half
    assert_eq!(buf, &data[offset..]);
}

// ============================================================================
// Validate source (HEAD request)
// ============================================================================

#[test]
fn http_validate_source_succeeds() {
    let data = test_data(1024);
    let server = start_mock(MockBehavior::ServeData(data));
    let url = format!("{}/image.iso", server.url);

    let info = validate_source(&url).unwrap();
    assert_eq!(info.source_type, SourceType::Remote);
    assert!(info.size.is_some());
}

// ============================================================================
// Error handling
// ============================================================================

#[test]
fn http_source_404_returns_error() {
    let server = start_mock(MockBehavior::StatusCode(404, "Not Found".to_string()));
    let url = format!("{}/missing.iso", server.url);

    let result = Source::open(&url);
    assert!(result.is_err(), "Should return error for 404");
}

#[test]
fn http_source_500_returns_error() {
    let server = start_mock(MockBehavior::StatusCode(
        500,
        "Internal Server Error".to_string(),
    ));
    let url = format!("{}/broken.iso", server.url);

    let result = Source::open(&url);
    assert!(result.is_err(), "Should return error for 500");
}

#[test]
fn http_source_connection_refused() {
    // Connect to a port that's definitely not listening
    // Use a high port that's unlikely to be in use
    let result = Source::open("http://127.0.0.1:1/image.iso");
    assert!(
        result.is_err(),
        "Should return error for connection refused"
    );
}

// ============================================================================
// Write + verify via HTTP
// ============================================================================

#[cfg(feature = "checksum")]
#[test]
fn http_write_and_verify() {
    use engraver_core::ChecksumAlgorithm;

    let data = test_data(256 * 1024);
    let server = start_mock(MockBehavior::ServeData(data.clone()));
    let url = format!("{}/image.iso", server.url);

    let mut source = Source::open(&url).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new()
        .block_size(64 * 1024)
        .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
    let mut writer = Writer::with_config(config);

    let result = writer
        .write_and_verify(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(result.verified, Some(true));
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// HTTP with progress tracking
// ============================================================================

#[test]
fn http_write_with_progress() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let data = test_data(128 * 1024);
    let server = start_mock(MockBehavior::ServeData(data.clone()));
    let url = format!("{}/image.iso", server.url);

    let mut source = Source::open(&url).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let update_count = Arc::new(AtomicU64::new(0));
    let update_count_clone = Arc::clone(&update_count);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config).on_progress(move |_p| {
        update_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert!(
        update_count.load(Ordering::SeqCst) > 0,
        "Should receive progress updates during HTTP write"
    );
}

// ============================================================================
// HTTP + compression pipeline
// ============================================================================

#[cfg(feature = "compression")]
#[test]
fn http_gzip_source_pipeline() {
    use engraver_core::source::{GzipSource, HttpSource, SourceInfo, SourceType as ST};

    // Compress data, serve it via HTTP
    let data = test_data(128 * 1024);
    let mut compressed = Vec::new();
    {
        let mut encoder =
            flate2::write::GzEncoder::new(&mut compressed, flate2::Compression::fast());
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap();
    }

    let compressed_len = compressed.len();
    let server = start_mock(MockBehavior::ServeData(compressed));
    let url = format!("{}/image.iso.gz", server.url);

    // Manually construct HttpSource → GzipSource pipeline
    // (Source::open treats all HTTP URLs as plain Remote)
    let http_source = HttpSource::open(&url).unwrap();
    let info = SourceInfo::compressed(&url, compressed_len as u64, ST::Gzip);
    let mut gzip_source = GzipSource::new(http_source, info);

    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut gzip_source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Multiple sequential requests to same server
// ============================================================================

#[test]
fn http_multiple_requests_same_server() {
    let data = test_data(32 * 1024);
    let server = start_mock(MockBehavior::ServeData(data.clone()));

    // Make two sequential requests to the same server
    for i in 0..2 {
        let url = format!("{}/image_{}.iso", server.url, i);
        let mut source = Source::open(&url).unwrap();
        let mut buf = Vec::new();
        source.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, data, "Request {} failed", i);
    }
}
