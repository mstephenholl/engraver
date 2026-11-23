//! Integration tests for compressed images fetched from a URL
//!
//! A URL whose path ends in a compression extension must be decompressed on
//! the way to the target, the same as a local file with that extension. Each
//! test spins up its own `tiny_http` server on an OS-assigned port, so all
//! tests can run in parallel without port conflicts.

#![cfg(all(feature = "remote", feature = "compression"))]

use engraver_core::{
    validate_source, Error, Source, SourceType, WriteConfig, Writer, MIN_BLOCK_SIZE,
};
use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use tiny_http::{Header, Response, Server};

// ============================================================================
// Mock HTTP Server
// ============================================================================

struct MockServer {
    url: String,
    _server: Arc<Server>,
}

/// Serve `body` at every path with a `Content-Length` and `Accept-Ranges: bytes`.
/// The thread runs until the test process exits.
fn serve(body: Vec<u8>) -> MockServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let url = format!("http://{}", server.server_addr().to_ip().unwrap());

    let handle = Arc::clone(&server);
    std::thread::spawn(move || {
        while let Ok(request) = handle.recv() {
            let response = Response::from_data(body.clone())
                .with_chunked_threshold(usize::MAX)
                .with_header(Header::from_bytes(b"Accept-Ranges", b"bytes" as &[u8]).unwrap());
            let _ = request.respond(response);
        }
    });

    MockServer {
        url,
        _server: server,
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// A known image: several blocks and a partial one, so the last block is short.
fn test_image() -> Vec<u8> {
    (0..16 * MIN_BLOCK_SIZE + 123)
        .map(|i| (i % 251) as u8)
        .collect()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn xz(data: &[u8]) -> Vec<u8> {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 1);
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn zstd(data: &[u8]) -> Vec<u8> {
    zstd::encode_all(data, 1).unwrap()
}

fn bzip2(data: &[u8]) -> Vec<u8> {
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

type Compress = fn(&[u8]) -> Vec<u8>;

/// Every supported compression: the URL extension and a real encoder for it.
const FORMATS: [(&str, Compress); 4] = [("gz", gzip), ("xz", xz), ("zst", zstd), ("bz2", bzip2)];

fn serve_compressed(image: &[u8], compress: Compress) -> (MockServer, u64) {
    let compressed = compress(image);
    let len = compressed.len() as u64;
    (serve(compressed), len)
}

fn read_all(source: &mut Source) -> Vec<u8> {
    let mut buf = Vec::new();
    source.read_to_end(&mut buf).unwrap();
    buf
}

// ============================================================================
// A compressed URL is decompressed
// ============================================================================

#[test]
fn compressed_url_reads_back_the_decompressed_image() {
    let image = test_image();

    for (ext, compress) in FORMATS {
        let (server, _) = serve_compressed(&image, compress);
        let url = format!("{}/image.img.{ext}", server.url);

        let mut source = Source::open(&url).unwrap();

        assert!(
            read_all(&mut source) == image,
            ".{ext} URL did not read back the decompressed image"
        );
    }
}

#[test]
fn compressed_url_writes_the_decompressed_image() {
    let image = test_image();

    for (ext, compress) in FORMATS {
        let (server, _) = serve_compressed(&image, compress);
        let url = format!("{}/image.img.{ext}", server.url);

        let mut source = Source::open(&url).unwrap();
        // The size the CLI hands the Writer: the image length, or 0 when unknown.
        let source_size = source.info().size.unwrap_or(0);
        let mut target = Cursor::new(vec![0u8; image.len()]);

        let result = Writer::with_config(WriteConfig::new().block_size(MIN_BLOCK_SIZE))
            .write(&mut source, &mut target, source_size)
            .unwrap_or_else(|e| panic!(".{ext} URL failed to write: {e}"));

        assert_eq!(result.bytes_written, image.len() as u64, ".{ext}");
        assert!(
            target.into_inner() == image,
            ".{ext} URL did not write the decompressed image"
        );
    }
}

#[test]
fn compressed_url_is_reported_as_compressed() {
    let image = test_image();

    for (ext, compress) in FORMATS {
        let (server, _) = serve_compressed(&image, compress);
        let source = Source::open(&format!("{}/image.img.{ext}", server.url)).unwrap();

        assert!(source.is_compressed(), ".{ext}");
    }
}

// ============================================================================
// Detection reads the URL path, not the whole URL
// ============================================================================

#[test]
fn query_string_does_not_hide_the_extension() {
    let image = test_image();
    let (server, _) = serve_compressed(&image, gzip);

    for suffix in [
        "/image.img.gz?download=1",
        "/image.img.gz?a=1&b=2",
        "/IMAGE.IMG.GZ",
        "/image.img.gz#section",
    ] {
        let mut source = Source::open(&format!("{}{suffix}", server.url)).unwrap();

        assert!(
            read_all(&mut source) == image,
            "{suffix} did not read back the decompressed image"
        );
    }
}

#[test]
fn compression_extension_in_the_query_does_not_make_a_url_compressed() {
    let image = test_image();
    let server = serve(image.clone());

    for suffix in [
        "/image.img?file=disk.gz",
        "/image.img?file=disk.xz",
        "/image.img#disk.zst",
    ] {
        let mut source = Source::open(&format!("{}{suffix}", server.url)).unwrap();

        assert!(
            read_all(&mut source) == image,
            "{suffix} was decompressed but its path is uncompressed"
        );
    }
}

// ============================================================================
// An uncompressed URL is read as is
// ============================================================================

#[test]
fn uncompressed_url_is_unchanged() {
    let image = test_image();
    let server = serve(image.clone());
    let url = format!("{}/image.img", server.url);

    let mut source = Source::open(&url).unwrap();

    assert!(!source.is_compressed());
    assert_eq!(source.info().size, Some(image.len() as u64));
    assert!(source.info().resumable, "the server sends Accept-Ranges");
    assert!(read_all(&mut source) == image);
}

#[test]
fn validate_source_reports_the_length_of_an_uncompressed_url() {
    let image = test_image();
    let server = serve(image.clone());

    let info = validate_source(&format!("{}/image.img", server.url)).unwrap();

    assert_eq!(info.size, Some(image.len() as u64));
    assert_eq!(info.compressed_size, Some(image.len() as u64));
    assert!(info.resumable, "the server sends Accept-Ranges");
}

// ============================================================================
// Source info for a compressed URL
// ============================================================================

#[test]
fn validate_source_reports_unknown_decompressed_size() {
    let image = test_image();

    for (ext, compress) in FORMATS {
        let (server, compressed_len) = serve_compressed(&image, compress);
        let url = format!("{}/image.img.{ext}", server.url);

        let info = validate_source(&url).unwrap();

        assert_eq!(info.source_type, SourceType::Remote, ".{ext}");
        assert_eq!(info.size, None, ".{ext}: decompressed length is unknown");
        assert_eq!(info.compressed_size, Some(compressed_len), ".{ext}");
        assert!(
            !info.resumable,
            ".{ext}: Range offsets are compressed bytes"
        );
    }
}

#[test]
fn opened_source_info_agrees_with_validate_source() {
    let image = test_image();

    for (ext, compress) in FORMATS {
        let (server, compressed_len) = serve_compressed(&image, compress);
        let url = format!("{}/image.img.{ext}", server.url);

        let validated = validate_source(&url).unwrap();
        let source = Source::open(&url).unwrap();
        let opened = source.info();

        assert_eq!(opened.size, validated.size, ".{ext}");
        assert_eq!(opened.compressed_size, validated.compressed_size, ".{ext}");
        assert_eq!(opened.resumable, validated.resumable, ".{ext}");
        assert_eq!(opened.size, None, ".{ext}");
        assert_eq!(opened.compressed_size, Some(compressed_len), ".{ext}");
        assert!(!opened.resumable, ".{ext}");
    }
}

// ============================================================================
// A compressed URL cannot resume
// ============================================================================

#[test]
fn compressed_url_cannot_resume_from_an_offset() {
    let image = test_image();

    for (ext, compress) in FORMATS {
        let (server, _) = serve_compressed(&image, compress);
        let url = format!("{}/image.img.{ext}", server.url);

        let err = Source::open_with_offset(&url, 4096).err();

        assert!(
            matches!(err, Some(Error::InvalidConfig(_))),
            ".{ext}: resuming a compressed URL must be an InvalidConfig error, got {err:?}"
        );
    }
}
