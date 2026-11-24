#![cfg(feature = "compression")]

use std::io::Write;

use engraver_core::{validate_source, Source, SourceInfo};

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
