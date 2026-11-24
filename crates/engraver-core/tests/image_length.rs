#![cfg(feature = "compression")]

use std::io::{Read, Write};

use engraver_core::{validate_source, Source, SourceInfo};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

fn image(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i % 251) as u8).wrapping_add(seed))
        .collect()
}

fn xz(image: &[u8]) -> Vec<u8> {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 1);
    encoder.write_all(image).unwrap();
    encoder.finish().unwrap()
}

fn xz_blocks(image: &[u8], block_size: u64) -> Vec<u8> {
    let stream = xz2::stream::MtStreamBuilder::new()
        .threads(1)
        .block_size(block_size)
        .preset(1)
        .encoder()
        .unwrap();
    let mut encoder = xz2::write::XzEncoder::new_stream(Vec::new(), stream);
    encoder.write_all(image).unwrap();
    encoder.finish().unwrap()
}

fn gzip(image: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(image).unwrap();
    encoder.finish().unwrap()
}

fn zstd_frame(image: &[u8], content_size: bool) -> Vec<u8> {
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    encoder.include_contentsize(content_size).unwrap();
    encoder
        .set_pledged_src_size(Some(image.len() as u64))
        .unwrap();
    encoder.write_all(image).unwrap();
    encoder.finish().unwrap()
}

fn bzip2(image: &[u8]) -> Vec<u8> {
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    encoder.write_all(image).unwrap();
    encoder.finish().unwrap()
}

fn agreed_lengths(ext: &str, compressed: &[u8]) -> (Option<u64>, Option<u64>) {
    let (validated, opened) = infos(ext, compressed);
    assert_eq!(
        (opened.size, opened.min_size),
        (validated.size, validated.min_size),
        ".{ext}: Source::open and validate_source agree"
    );
    (validated.size, validated.min_size)
}

fn decompress(ext: &str, compressed: &[u8]) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("image.img.{ext}"));
    std::fs::write(&path, compressed).unwrap();
    let mut image = Vec::new();
    Source::open(path.to_str().unwrap())
        .unwrap()
        .read_to_end(&mut image)
        .unwrap();
    image
}

fn infos(ext: &str, compressed: &[u8]) -> (SourceInfo, SourceInfo) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("image.img.{ext}"));
    std::fs::write(&path, compressed).unwrap();
    let path = path.to_str().unwrap();
    let validated = validate_source(path).unwrap();
    let opened = Source::open(path).unwrap().info().clone();
    (validated, opened)
}

fn assert_exact(ext: &str, compressed: &[u8], len: usize, case: &str) {
    let (validated, opened) = infos(ext, compressed);
    assert_eq!(validated.size, Some(len as u64), "{case}: validate_source");
    assert_eq!(opened.size, Some(len as u64), "{case}: Source::open");
}

#[test]
fn xz_reports_the_exact_image_length() {
    let one = image(300_000, 0);
    let two = image(70_001, 7);
    let concatenated = [xz(&one), xz(&two)].concat();
    let padded_between = [xz(&one), vec![0; 8], xz(&two)].concat();
    let padded_after = [xz(&one), vec![0; 12]].concat();

    assert_exact("xz", &xz(&one), one.len(), "one stream, one block");
    assert_exact("xz", &xz(&[]), 0, "an empty image");
    assert_exact("xz", &xz_blocks(&one, 64 * 1024), one.len(), "many blocks");
    assert_exact("xz", &concatenated, one.len() + two.len(), "two streams");
    assert_exact(
        "xz",
        &padded_between,
        one.len() + two.len(),
        "padding between streams",
    );
    assert_exact("xz", &padded_after, one.len(), "padding after the stream");
}

/// A sparse file ends in a hole of zeros, which the filesystem can skip without
/// reading. The size check has to do the same, or it reads the hole 4 KiB at a
/// time. The work runs on a thread so a slow check fails the test promptly.
#[cfg(unix)]
#[test]
fn xz_reports_the_exact_length_for_a_sparse_terabyte_of_padding() {
    use std::os::unix::fs::MetadataExt;
    use std::sync::mpsc;
    use std::time::Duration;

    const TIB: u64 = 1 << 40;
    let one = image(300_000, 0);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.img.xz");
    std::fs::write(&path, xz(&one)).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    if file.set_len(TIB).is_err() || file.metadata().unwrap().blocks() * 512 > 1024 * 1024 {
        eprintln!("skipped: this filesystem cannot hold a 1 TiB sparse file");
        return;
    }
    drop(file);

    let path = path.to_str().unwrap().to_owned();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let validated = validate_source(&path).unwrap();
        let opened = Source::open(&path).unwrap().info().clone();
        let _ = tx.send((validated.size, opened.size));
    });
    let sizes = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the size check did not finish within 10 s");
    let len = Some(one.len() as u64);
    assert_eq!(sizes, (len, len), "validate_source, Source::open");
}

#[test]
fn gzip_reports_its_last_member_as_a_lower_bound() {
    let one = image(300_000, 0);
    let two = image(70_001, 7);

    assert_eq!(
        agreed_lengths("gz", &gzip(&one)),
        (None, Some(one.len() as u64))
    );
    assert_eq!(
        agreed_lengths("gz", &[gzip(&one), gzip(&two)].concat()),
        (None, Some(two.len() as u64)),
        "two members"
    );
}

#[test]
fn gzip_zero_padding_only_lowers_the_bound() {
    let one = image(0x0001_0203, 0);
    for padding in 0..=9 {
        let padded = [gzip(&one), vec![0; padding]].concat();
        assert_eq!(
            decompress("gz", &padded),
            one,
            "engraver reads {padding} zero bytes as padding"
        );

        let (size, min_size) = agreed_lengths("gz", &padded);
        assert_eq!(size, None);
        let min_size = min_size.expect("a gzip file always gives a bound");
        assert!(
            min_size <= one.len() as u64,
            "{padding} bytes of padding: {min_size}"
        );
        if padding == 0 {
            assert_eq!(min_size, one.len() as u64);
        }
    }
}

#[test]
fn xz_with_a_corrupt_footer_reports_nothing() {
    let compressed = xz(&image(300_000, 0));
    let footer = compressed.len() - 12;
    for at in footer..compressed.len() {
        let mut corrupt = compressed.clone();
        corrupt[at] ^= 0x01;
        assert_eq!(
            agreed_lengths("xz", &corrupt),
            (None, None),
            "byte {} of the footer",
            at - footer
        );
    }
}

#[test]
fn xz_with_a_corrupt_header_or_index_reports_nothing() {
    let compressed = xz(&image(300_000, 0));
    let index_crc = compressed.len() - 12 - 4;
    for at in [0, 5, 6, 8, index_crc] {
        let mut corrupt = compressed.clone();
        corrupt[at] ^= 0x01;
        assert_eq!(agreed_lengths("xz", &corrupt), (None, None), "byte {at}");
    }
}

#[test]
fn xz_padding_that_is_not_a_multiple_of_four_reports_nothing() {
    for padding in [1, 2, 3, 5] {
        let padded = [xz(&image(1000, 0)), vec![0; padding]].concat();
        assert_eq!(
            agreed_lengths("xz", &padded),
            (None, None),
            "{padding} bytes"
        );
    }
}

#[test]
fn xz_with_an_unreadable_earlier_stream_reports_the_later_ones_as_a_lower_bound() {
    let one = image(300_000, 0);
    let two = image(70_001, 7);
    let first = xz(&one);
    let mut corrupt = [first.clone(), xz(&two)].concat();
    corrupt[first.len() - 1] ^= 0x01;

    assert_eq!(
        agreed_lengths("xz", &corrupt),
        (None, Some(two.len() as u64))
    );
}

#[test]
fn zstd_reports_its_first_frame_content_size_as_a_lower_bound() {
    let one = image(300_000, 0);
    let two = image(70_001, 7);

    assert_eq!(
        agreed_lengths("zst", &zstd_frame(&one, true)),
        (None, Some(one.len() as u64))
    );
    assert_eq!(
        agreed_lengths(
            "zst",
            &[zstd_frame(&one, true), zstd_frame(&two, true)].concat()
        ),
        (None, Some(one.len() as u64)),
        "two frames"
    );
    assert_eq!(
        agreed_lengths("zst", &zstd_frame(&one, false)),
        (None, None),
        "a frame with no content size"
    );
}

#[test]
fn bzip2_reports_nothing() {
    assert_eq!(
        agreed_lengths("bz2", &bzip2(&image(300_000, 0))),
        (None, None)
    );
}

#[test]
fn a_file_that_is_not_what_its_extension_says_reports_nothing() {
    let plain = image(300_000, 0);
    for ext in ["gz", "xz", "zst"] {
        assert_eq!(agreed_lengths(ext, &plain), (None, None), ".{ext}");
        assert_eq!(agreed_lengths(ext, &[]), (None, None), "an empty .{ext}");
    }
}

fn random_image(rng: &mut StdRng) -> Vec<u8> {
    let mut image = Vec::new();
    for _ in 0..rng.random_range(0..4) {
        let run = rng.random_range(0..32 * 1024);
        match rng.random_range(0..3) {
            0 => image.extend(std::iter::repeat_n(0, run)),
            1 => image.extend((0..run).map(|i| (i % 251) as u8)),
            _ => image.extend((0..run).map(|_| rng.random::<u8>())),
        }
    }
    image
}

#[test]
fn sweep_reported_lengths_are_never_above_the_image() {
    let mut rng = StdRng::seed_from_u64(0x1e46_7468);
    let (mut exact, mut lower_bounds) = (0, 0);

    for case in 0..300 {
        let parts: Vec<Vec<u8>> = (0..rng.random_range(1..=3))
            .map(|_| random_image(&mut rng))
            .collect();
        let len = parts.iter().map(Vec::len).sum::<usize>() as u64;

        let (ext, compressed) = match case % 3 {
            0 => {
                let mut members: Vec<u8> = parts.iter().flat_map(|p| gzip(p)).collect();
                members.extend(std::iter::repeat_n(0, rng.random_range(0..10)));
                ("gz", members)
            }
            1 => {
                let mut streams = Vec::new();
                for part in &parts {
                    let block_size = rng.random_range(4 * 1024..64 * 1024);
                    streams.extend(xz_blocks(part, block_size));
                    streams.extend(std::iter::repeat_n(0, 4 * rng.random_range(0..3)));
                }
                ("xz", streams)
            }
            _ => {
                let frames = parts
                    .iter()
                    .flat_map(|p| zstd_frame(p, rng.random_bool(0.7)))
                    .collect();
                ("zst", frames)
            }
        };
        assert_eq!(
            decompress(ext, &compressed).len() as u64,
            len,
            "case {case}: the file is a valid .{ext}"
        );

        match agreed_lengths(ext, &compressed) {
            (Some(size), None) => {
                assert_eq!(size, len, "case {case}: exact .{ext} length");
                exact += 1;
            }
            (None, Some(min_size)) => {
                assert!(
                    min_size <= len,
                    "case {case}: .{ext} bound {min_size} > {len}"
                );
                lower_bounds += 1;
            }
            (None, None) => {}
            both => panic!("case {case}: .{ext} reports both {both:?}"),
        }
    }

    assert!(
        exact >= 100 && lower_bounds >= 150,
        "{exact} exact, {lower_bounds} bounds"
    );
}
