//! Integration tests for resuming an HTTP source with a Range request
//!
//! A resume sends `Range: bytes=N-` and writes whatever comes back at offset
//! N of the target. If the reply does not start at byte N, those bytes land
//! in the wrong place, so opening the source must fail before any of them
//! are read.
//!
//! Each test spins up its own `tiny_http` server on an OS-assigned port, so
//! all tests can run in parallel without port conflicts.

#![cfg(feature = "remote")]

use engraver_core::{Error, Source};
use std::io::{Cursor, Read};
use std::sync::Arc;
use tiny_http::{Header, Response, Server, StatusCode};

// ============================================================================
// Mock HTTP Server
// ============================================================================

type Reply = Response<Cursor<Vec<u8>>>;

/// A mock HTTP server on a background thread that runs until the test process exits.
struct MockHttpServer {
    url: String,
    _server: Arc<Server>,
}

/// Start a server that answers every request with `reply`.
///
/// `reply` receives N from the request's `Range: bytes=N-` header, or `None`
/// when the request has no Range header.
fn start_mock(reply: impl Fn(Option<usize>) -> Reply + Send + 'static) -> MockHttpServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let url = format!("http://{}/image.iso", server.server_addr().to_ip().unwrap());

    let server_clone = Arc::clone(&server);
    std::thread::spawn(move || {
        while let Ok(request) = server_clone.recv() {
            let range_start = request
                .headers()
                .iter()
                .find(|h| h.field.equiv("Range"))
                .and_then(|h| h.value.as_str().strip_prefix("bytes="))
                .and_then(|r| r.strip_suffix('-'))
                .and_then(|n| n.parse::<usize>().ok());
            let _ = request.respond(reply(range_start));
        }
    });

    MockHttpServer {
        url,
        _server: server,
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

/// A 200 reply carrying the whole image, whatever range was asked for.
fn full_reply(data: &[u8]) -> Reply {
    Response::from_data(data.to_vec())
}

/// A 206 reply whose body is `data[body_start..]`, with `content_range` as
/// its Content-Range header (or none).
fn partial_reply(data: &[u8], body_start: usize, content_range: Option<String>) -> Reply {
    let mut response = Response::from_data(data[body_start..].to_vec())
        .with_status_code(StatusCode(206))
        .with_header(header("Accept-Ranges", "bytes"));
    if let Some(value) = content_range {
        response = response.with_header(header("Content-Range", &value));
    }
    response
}

// ============================================================================
// Helpers
// ============================================================================

const IMAGE_SIZE: usize = 128 * 1024;
const OFFSET: usize = 64 * 1024;

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

/// Open `url` for a resume at `offset`, expecting it to be refused.
///
/// If the open succeeds, the test fails and reports how many bytes the
/// source would have handed to the writer.
fn refused_resume(url: &str, offset: usize) -> Error {
    match Source::open_with_offset(url, offset as u64) {
        Ok(mut source) => {
            let mut buf = Vec::new();
            let read = source.read_to_end(&mut buf).unwrap();
            panic!("resume at {offset} was accepted and would write {read} bytes at that offset");
        }
        Err(e) => e,
    }
}

fn assert_refusal_explains_range(err: &Error) {
    assert!(
        err.to_string().contains("Range"),
        "the error should tell the user the Range request failed, got: {err}"
    );
}

// ============================================================================
// A server that ignores the Range header
// ============================================================================

#[test]
fn resume_is_refused_when_server_ignores_range() {
    let data = test_data(IMAGE_SIZE);
    let server = start_mock(move |_| full_reply(&data));

    let err = refused_resume(&server.url, OFFSET);

    assert_refusal_explains_range(&err);
}

// ============================================================================
// A 206 reply that does not start at the offset
// ============================================================================

#[test]
fn resume_is_refused_when_content_range_starts_elsewhere() {
    let last = IMAGE_SIZE - 1;
    // The server answers from byte 0, just before the offset, or past it.
    for wrong_start in [0, OFFSET - 1, OFFSET + 1, OFFSET + 4096] {
        let data = test_data(IMAGE_SIZE);
        let server = start_mock(move |_| {
            let range = format!("bytes {wrong_start}-{last}/{IMAGE_SIZE}");
            partial_reply(&data, wrong_start, Some(range))
        });

        let err = refused_resume(&server.url, OFFSET);

        assert_refusal_explains_range(&err);
    }
}

#[test]
fn resume_is_refused_when_content_range_is_missing() {
    let data = test_data(IMAGE_SIZE);
    let server = start_mock(move |start| partial_reply(&data, start.unwrap_or(0), None));

    let err = refused_resume(&server.url, OFFSET);

    assert_refusal_explains_range(&err);
}

#[test]
fn resume_is_refused_when_content_range_is_garbled() {
    let garbled = [
        "garbage",
        "bytes",
        "bytes abc-131071/131072",
        "bytes -131071/131072",
        "bytes 65536",
        "items 65536-131071/131072",
    ];
    for content_range in garbled {
        let data = test_data(IMAGE_SIZE);
        let server = start_mock(move |start| {
            partial_reply(&data, start.unwrap_or(0), Some(content_range.to_string()))
        });

        let err = refused_resume(&server.url, OFFSET);

        assert!(
            err.to_string().contains("Range"),
            "Content-Range {content_range:?} should be refused with a Range message, got: {err}"
        );
    }
}

// ============================================================================
// Replies that must keep working
// ============================================================================

#[test]
fn resume_reads_exactly_the_rest_of_the_image() {
    let data = test_data(IMAGE_SIZE);
    let image = data.clone();
    let server = start_mock(move |start| match start {
        Some(start) => {
            let range = format!("bytes {start}-{}/{}", image.len() - 1, image.len());
            partial_reply(&image, start, Some(range))
        }
        None => full_reply(&image),
    });

    let mut source = Source::open_with_offset(&server.url, OFFSET as u64).unwrap();
    let mut buf = Vec::new();
    source.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, &data[OFFSET..]);
}

#[test]
fn offset_zero_accepts_a_200_reply() {
    // No Range header is sent at offset 0, so a plain 200 is the right answer.
    let data = test_data(IMAGE_SIZE);
    let image = data.clone();
    let server = start_mock(move |_| full_reply(&image));

    let mut source = Source::open_with_offset(&server.url, 0).unwrap();
    let mut buf = Vec::new();
    source.read_to_end(&mut buf).unwrap();

    assert_eq!(buf, data);
}
