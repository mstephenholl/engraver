//! Fuzz test for source type detection
//!
//! Tests that source type detection handles arbitrary inputs safely.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Test detect_source_type - should never panic
    let source_type = detect_source_type(data);

    // Verify properties are consistent
    let _ = source_type.is_compressed();
    let _ = source_type.is_remote();
    let _ = source_type.extension();

    // Test that Remote detection is correct
    if data.starts_with("http://") || data.starts_with("https://") {
        assert!(
            source_type.is_remote(),
            "URLs should be detected as Remote"
        );
    }

    // Test compression detection by extension
    let lower = data.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        if lower.ends_with(".gz") || lower.ends_with(".gzip") {
            assert!(
                matches!(source_type, SourceType::Gzip),
                "Should detect gzip"
            );
        }
        if lower.ends_with(".xz") {
            assert!(matches!(source_type, SourceType::Xz), "Should detect xz");
        }
        if lower.ends_with(".zst") || lower.ends_with(".zstd") {
            assert!(
                matches!(source_type, SourceType::Zstd),
                "Should detect zstd"
            );
        }
        if lower.ends_with(".bz2") || lower.ends_with(".bzip2") {
            assert!(
                matches!(source_type, SourceType::Bzip2),
                "Should detect bzip2"
            );
        }
    }
});

/// Source type detection replica for fuzzing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    LocalFile,
    Remote,
    Gzip,
    Xz,
    Zstd,
    Bzip2,
}

impl SourceType {
    pub fn is_compressed(&self) -> bool {
        matches!(
            self,
            SourceType::Gzip | SourceType::Xz | SourceType::Zstd | SourceType::Bzip2
        )
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, SourceType::Remote)
    }

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

fn detect_source_type(path: &str) -> SourceType {
    if path.starts_with("http://") || path.starts_with("https://") {
        return SourceType::Remote;
    }

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
