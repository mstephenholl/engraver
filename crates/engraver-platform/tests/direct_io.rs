#![cfg(any(target_os = "linux", target_os = "windows"))]

use engraver_platform::{open_device, OpenOptions, PlatformError, RawDevice};
use std::io::{Read, Write};

const BLOCK: usize = 4096;

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// A file holding `contents`, kept under the target directory because tmpfs
/// before Linux 6.6 rejects `O_DIRECT`.
fn file_with(contents: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("device.img");
    std::fs::write(&path, contents).unwrap();
    let path = path.to_str().unwrap().to_string();
    (dir, path)
}

fn open_direct(path: &str, write: bool) -> Option<Box<dyn RawDevice>> {
    let options = OpenOptions::new()
        .read(true)
        .write(write)
        .direct_io(true)
        .block_size(BLOCK);
    match open_device(path, options) {
        Ok(device) => Some(device),
        Err(PlatformError::Io(e))
            if cfg!(target_os = "linux") && e.kind() == std::io::ErrorKind::InvalidInput =>
        {
            eprintln!("skipped: the target directory's filesystem does not support O_DIRECT");
            None
        }
        Err(e) => panic!("open with direct I/O failed: {e}"),
    }
}

fn misaligned(storage: &mut [u8], len: usize) -> &mut [u8] {
    let start = storage.as_ptr().align_offset(BLOCK) + 1;
    &mut storage[start..start + len]
}

fn assert_same_bytes(actual: &[u8], expected: &[u8]) {
    if let Some(i) = (0..actual.len().min(expected.len())).find(|&i| actual[i] != expected[i]) {
        panic!(
            "byte {i} differs: got {:#04x}, expected {:#04x}",
            actual[i], expected[i]
        );
    }
    assert_eq!(actual.len(), expected.len(), "lengths differ");
}

#[test]
fn reads_of_1000_bytes_return_the_whole_file_in_order() {
    let contents = pattern(5 * BLOCK + 123);
    let (_dir, path) = file_with(&contents);
    let Some(mut device) = open_direct(&path, false) else {
        return;
    };

    let mut read_back = Vec::new();
    let mut chunk = [0u8; 1000];
    loop {
        let n = device.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        read_back.extend_from_slice(&chunk[..n]);
    }

    assert_same_bytes(&read_back, &contents);
}

#[test]
fn reads_into_misaligned_slices_of_unaligned_lengths_return_the_file() {
    let contents = pattern(6 * BLOCK + 500);
    let (_dir, path) = file_with(&contents);
    let Some(mut device) = open_direct(&path, false) else {
        return;
    };

    let mut storage = vec![0u8; contents.len() + 2 * BLOCK];
    let mut read_back = Vec::new();
    for len in [
        2 * BLOCK + 1000,
        3000,
        BLOCK,
        contents.len() - 3 * BLOCK - 4000,
    ] {
        let buf = misaligned(&mut storage, len);
        device.read_exact(buf).unwrap();
        read_back.extend_from_slice(buf);
    }

    assert_same_bytes(&read_back, &contents);
}

#[test]
fn read_at_an_unaligned_offset_returns_the_bytes_there() {
    let contents = pattern(3 * BLOCK);
    let (_dir, path) = file_with(&contents);
    let Some(mut device) = open_direct(&path, false) else {
        return;
    };

    for offset in [1, 511, 700] {
        let mut buf = [0u8; 1000];
        let n = device.read_at(offset as u64, &mut buf).unwrap();
        assert_same_bytes(&buf[..n], &contents[offset..offset + buf.len()]);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn unaligned_writes_in_a_row_never_leave_a_gap() {
    let (_dir, path) = file_with(&[0xFF; 3 * BLOCK]);
    let Some(mut device) = open_direct(&path, true) else {
        return;
    };
    let data = pattern(2000);

    device.write_all(&data[..1000]).unwrap();
    let second = device.write_all(&data[1000..]);
    drop(device);

    let on_disk = std::fs::read(&path).unwrap();
    match second {
        Ok(()) => assert_same_bytes(&on_disk[..2000], &data),
        Err(e) => {
            assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
            assert_same_bytes(&on_disk[..1000], &data[..1000]);
        }
    }
}
