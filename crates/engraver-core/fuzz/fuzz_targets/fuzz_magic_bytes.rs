//! Fuzz test for compression magic byte detection
//!
//! Tests that magic byte detection handles arbitrary byte sequences safely.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Test detect_compression_from_magic - should never panic
    let result = detect_compression_from_magic(data);

    // Verify known magic bytes are detected correctly
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        assert!(
            matches!(result, Some(CompressionType::Gzip)),
            "Gzip magic should be detected"
        );
    }

    if data.len() >= 6
        && data[0] == 0xfd
        && data[1] == 0x37
        && data[2] == 0x7a
        && data[3] == 0x58
        && data[4] == 0x5a
        && data[5] == 0x00
    {
        assert!(
            matches!(result, Some(CompressionType::Xz)),
            "XZ magic should be detected"
        );
    }

    if data.len() >= 4
        && data[0] == 0x28
        && data[1] == 0xb5
        && data[2] == 0x2f
        && data[3] == 0xfd
    {
        assert!(
            matches!(result, Some(CompressionType::Zstd)),
            "Zstd magic should be detected"
        );
    }

    if data.len() >= 3 && data[0] == 0x42 && data[1] == 0x5a && data[2] == 0x68 {
        assert!(
            matches!(result, Some(CompressionType::Bzip2)),
            "Bzip2 magic should be detected"
        );
    }

    // Short data should return None
    if data.len() < 2 {
        assert!(result.is_none(), "Too short data should return None");
    }
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    Gzip,
    Xz,
    Zstd,
    Bzip2,
}

fn detect_compression_from_magic(bytes: &[u8]) -> Option<CompressionType> {
    if bytes.len() < 2 {
        return None;
    }

    // Gzip: 1f 8b
    if bytes[0] == 0x1f && bytes[1] == 0x8b {
        return Some(CompressionType::Gzip);
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
        return Some(CompressionType::Xz);
    }

    // Zstd: 28 b5 2f fd
    if bytes.len() >= 4
        && bytes[0] == 0x28
        && bytes[1] == 0xb5
        && bytes[2] == 0x2f
        && bytes[3] == 0xfd
    {
        return Some(CompressionType::Zstd);
    }

    // Bzip2: 42 5a 68 (BZh)
    if bytes.len() >= 3 && bytes[0] == 0x42 && bytes[1] == 0x5a && bytes[2] == 0x68 {
        return Some(CompressionType::Bzip2);
    }

    None
}
