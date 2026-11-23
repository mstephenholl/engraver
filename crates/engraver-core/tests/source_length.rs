//! Integration tests for the Writer's source-length contract

use engraver_core::{Error, WriteConfig, Writer, MIN_BLOCK_SIZE};
use std::io::Cursor;

// ============================================================================
// Helpers
// ============================================================================

const SENTINEL: u8 = 0xEE;

const PRIME_PERIOD: usize = 251;

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % PRIME_PERIOD) as u8).collect()
}

fn multi_block_writer() -> Writer {
    Writer::with_config(WriteConfig::new().block_size(MIN_BLOCK_SIZE))
}

fn sentinel_target(len: usize) -> Cursor<Vec<u8>> {
    Cursor::new(vec![SENTINEL; len])
}

fn assert_untouched_from(target: &Cursor<Vec<u8>>, from: usize) {
    assert!(
        target.get_ref()[from..].iter().all(|&b| b == SENTINEL),
        "target was written at or past offset {from}, the end of the image"
    );
}

// ============================================================================
// A source shorter than source_size
// ============================================================================

#[test]
fn short_source_is_an_error() {
    let cases = [
        (1_000, 1_000_000),                       // truncated download
        (10_000, 20_000),                         // ends inside a block
        (2 * MIN_BLOCK_SIZE, 3 * MIN_BLOCK_SIZE), // ends exactly on a block boundary
        (0, 4_096),                               // empty source
    ];

    for (actual, expected) in cases {
        let data = test_data(actual);
        let mut target = sentinel_target(expected);

        let result = multi_block_writer().write(Cursor::new(data), &mut target, expected as u64);

        assert!(
            matches!(
                result,
                Err(Error::SourceTooShort { expected: e, actual: a })
                    if e == expected as u64 && a == actual as u64
            ),
            "source of {actual} bytes with source_size {expected} must fail as too short, got {result:?}"
        );
    }
}

// ============================================================================
// A source longer than source_size
// ============================================================================

#[test]
fn long_source_is_an_error() {
    let cases = [
        (4 * MIN_BLOCK_SIZE, 2 * MIN_BLOCK_SIZE), // overrun by whole blocks
        (12_000, 10_000),                         // overrun inside a block
        (2 * MIN_BLOCK_SIZE + 1, 2 * MIN_BLOCK_SIZE), // overrun by one byte
    ];

    for (actual, expected) in cases {
        let data = test_data(actual);
        let mut target = sentinel_target(actual + MIN_BLOCK_SIZE);

        let result = multi_block_writer().write(Cursor::new(data), &mut target, expected as u64);

        assert!(
            matches!(result, Err(Error::SourceTooLong { expected: e }) if e == expected as u64),
            "source of {actual} bytes with source_size {expected} must fail as too long, got {result:?}"
        );
    }
}

#[test]
fn long_source_writes_nothing_past_source_size() {
    let cases = [
        (4 * MIN_BLOCK_SIZE, 2 * MIN_BLOCK_SIZE),
        (12_000, 10_000),
        (2 * MIN_BLOCK_SIZE + 1, 2 * MIN_BLOCK_SIZE),
    ];

    for (actual, expected) in cases {
        let data = test_data(actual);
        let mut target = sentinel_target(actual + MIN_BLOCK_SIZE);

        let _ = multi_block_writer().write(Cursor::new(data), &mut target, expected as u64);

        assert_untouched_from(&target, expected);
    }
}

#[test]
fn source_matching_source_size_succeeds() {
    for len in [1, 1_000, MIN_BLOCK_SIZE, 10_000, 3 * MIN_BLOCK_SIZE] {
        let data = test_data(len);
        let mut target = sentinel_target(len + MIN_BLOCK_SIZE);

        let result = multi_block_writer()
            .write(Cursor::new(data.clone()), &mut target, len as u64)
            .unwrap();

        assert_eq!(result.bytes_written, len as u64);
        assert_eq!(&target.get_ref()[..len], &data[..]);
        assert_untouched_from(&target, len);
    }
}

// ============================================================================
// source_size = 0 means "unknown"
// ============================================================================

#[test]
fn unknown_source_size_writes_any_length() {
    for len in [0, 1, MIN_BLOCK_SIZE, 10_000, 3 * MIN_BLOCK_SIZE] {
        let data = test_data(len);
        let mut target = sentinel_target(len + MIN_BLOCK_SIZE);

        let result = multi_block_writer()
            .write(Cursor::new(data.clone()), &mut target, 0)
            .unwrap();

        assert_eq!(result.bytes_written, len as u64);
        assert_eq!(&target.get_ref()[..len], &data[..]);
    }
}

// ============================================================================
// Resume with write_from_offset
// ============================================================================

const IMAGE_LEN: usize = 16 * MIN_BLOCK_SIZE;

const RESUME_AT: usize = 4 * MIN_BLOCK_SIZE;

fn partially_written_target(image: &[u8], len: usize) -> Cursor<Vec<u8>> {
    let mut bytes = vec![SENTINEL; len];
    bytes[..RESUME_AT].copy_from_slice(&image[..RESUME_AT]);
    Cursor::new(bytes)
}

#[test]
fn resume_with_the_full_source_size_succeeds() {
    let image = test_data(IMAGE_LEN);
    let mut target = partially_written_target(&image, IMAGE_LEN + MIN_BLOCK_SIZE);

    let result = multi_block_writer()
        .write_from_offset(
            Cursor::new(image[RESUME_AT..].to_vec()),
            &mut target,
            IMAGE_LEN as u64,
            RESUME_AT as u64,
        )
        .unwrap();

    assert_eq!(result.bytes_written, IMAGE_LEN as u64);
    assert_eq!(&target.get_ref()[..IMAGE_LEN], &image[..]);
    assert_untouched_from(&target, IMAGE_LEN);
}

#[test]
fn resumed_source_ending_early_is_an_error() {
    let image = test_data(IMAGE_LEN);
    let mut target = partially_written_target(&image, IMAGE_LEN);

    let truncated = image[RESUME_AT..IMAGE_LEN - MIN_BLOCK_SIZE].to_vec();
    let result = multi_block_writer().write_from_offset(
        Cursor::new(truncated),
        &mut target,
        IMAGE_LEN as u64,
        RESUME_AT as u64,
    );

    assert!(
        matches!(result, Err(Error::SourceTooShort { expected, .. }) if expected == IMAGE_LEN as u64),
        "truncated resume must fail as too short, got {result:?}"
    );
}

#[test]
fn resumed_source_running_past_the_end_is_an_error() {
    let image = test_data(IMAGE_LEN);
    let mut target = partially_written_target(&image, IMAGE_LEN + MIN_BLOCK_SIZE);

    let mut overlong = image[RESUME_AT..].to_vec();
    overlong.extend(test_data(MIN_BLOCK_SIZE));
    let result = multi_block_writer().write_from_offset(
        Cursor::new(overlong),
        &mut target,
        IMAGE_LEN as u64,
        RESUME_AT as u64,
    );

    assert!(
        matches!(result, Err(Error::SourceTooLong { expected }) if expected == IMAGE_LEN as u64),
        "overlong resume must fail as too long, got {result:?}"
    );
    assert_untouched_from(&target, IMAGE_LEN);
}

#[test]
fn resume_from_a_server_that_ignores_range_is_an_error() {
    let image = test_data(IMAGE_LEN);
    let mut target = partially_written_target(&image, IMAGE_LEN + MIN_BLOCK_SIZE);

    let result = multi_block_writer().write_from_offset(
        Cursor::new(image.clone()),
        &mut target,
        IMAGE_LEN as u64,
        RESUME_AT as u64,
    );

    assert!(
        matches!(result, Err(Error::SourceTooLong { expected }) if expected == IMAGE_LEN as u64),
        "a source that ignored Range must fail as too long, got {result:?}"
    );
    assert_untouched_from(&target, IMAGE_LEN);
}

// ============================================================================
// write_and_verify reads back what was written
// ============================================================================

#[cfg(feature = "checksum")]
mod verify {
    use super::*;
    use engraver_core::ChecksumAlgorithm;

    fn checksumming_writer() -> Writer {
        Writer::with_config(
            WriteConfig::new()
                .block_size(MIN_BLOCK_SIZE)
                .checksum_algorithm(Some(ChecksumAlgorithm::Sha256)),
        )
    }

    #[test]
    fn unknown_source_size_verifies_what_was_written() {
        let data = test_data(3 * MIN_BLOCK_SIZE + 123);
        let mut target = Cursor::new(Vec::new());

        let result = checksumming_writer()
            .write_and_verify(Cursor::new(data.clone()), &mut target, 0)
            .unwrap();

        assert_eq!(result.bytes_written, data.len() as u64);
        assert_eq!(target.get_ref(), &data);
        assert_eq!(result.verified, Some(true));
        assert_eq!(result.source_checksum, result.target_checksum);
    }

    #[test]
    fn known_source_size_still_verifies() {
        let data = test_data(3 * MIN_BLOCK_SIZE + 123);
        let mut target = Cursor::new(Vec::new());

        let result = checksumming_writer()
            .write_and_verify(Cursor::new(data.clone()), &mut target, data.len() as u64)
            .unwrap();

        assert_eq!(result.verified, Some(true));
    }

    #[test]
    fn short_source_fails_instead_of_verifying() {
        let data = test_data(MIN_BLOCK_SIZE + 5);
        let mut target = sentinel_target(4 * MIN_BLOCK_SIZE);

        let result = checksumming_writer().write_and_verify(
            Cursor::new(data),
            &mut target,
            4 * MIN_BLOCK_SIZE as u64,
        );

        assert!(
            matches!(result, Err(Error::SourceTooShort { .. })),
            "short source must fail as too short, got {result:?}"
        );
    }
}

// ============================================================================
// A compressed source, as the CLI drives it
// ============================================================================

#[cfg(all(feature = "compression", feature = "checksum"))]
#[test]
fn gzip_source_write_and_verify_passes_with_unknown_size() {
    use engraver_core::{ChecksumAlgorithm, Source};
    use std::io::Write;

    let image = test_data(256 * 1024);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.img.gz");
    let mut encoder = flate2::write::GzEncoder::new(
        std::fs::File::create(&path).unwrap(),
        flate2::Compression::fast(),
    );
    encoder.write_all(&image).unwrap();
    encoder.finish().unwrap();

    let mut source = Source::open(path.to_str().unwrap()).unwrap();
    let source_info = source.info().clone();
    assert!(
        source_info.compressed_size.unwrap() < image.len() as u64,
        "test premise: the compressed size differs from the image length"
    );

    let mut target = Cursor::new(Vec::new());
    let mut writer = Writer::with_config(
        WriteConfig::new()
            .block_size(64 * 1024)
            .checksum_algorithm(Some(ChecksumAlgorithm::Sha256)),
    );

    let result = writer
        .write_and_verify(&mut source, &mut target, source_info.size.unwrap_or(0))
        .unwrap();

    assert_eq!(result.bytes_written, image.len() as u64);
    assert_eq!(target.get_ref(), &image);
    assert_eq!(result.verified, Some(true));
}
