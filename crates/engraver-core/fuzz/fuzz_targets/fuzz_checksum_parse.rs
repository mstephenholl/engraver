//! Fuzz test for checksum file parsing
//!
//! Tests that checksum file parsing handles arbitrary inputs safely.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Test parse_checksum_file - should never panic
    let entries = parse_checksum_file(data);

    // Validate all entries have non-empty checksums and filenames
    for entry in &entries {
        assert!(!entry.checksum.is_empty());
        assert!(!entry.filename.is_empty());

        // Algorithm should be valid if present
        if let Some(algo) = entry.algorithm {
            let _ = algo.name();
            let _ = algo.hex_length();
            let _ = algo.byte_length();
        }
    }

    // Test find_checksum_for_file with various patterns
    if !entries.is_empty() {
        let _ = find_checksum_for_file(&entries, "test.iso");
        let _ = find_checksum_for_file(&entries, &entries[0].filename);
        let _ = find_checksum_for_file(&entries, "/path/to/file");
    }
});

/// Checksum algorithm for testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    Sha256,
    Sha512,
    Md5,
    Crc32,
}

impl ChecksumAlgorithm {
    pub fn byte_length(&self) -> usize {
        match self {
            ChecksumAlgorithm::Sha256 => 32,
            ChecksumAlgorithm::Sha512 => 64,
            ChecksumAlgorithm::Md5 => 16,
            ChecksumAlgorithm::Crc32 => 4,
        }
    }

    pub fn hex_length(&self) -> usize {
        self.byte_length() * 2
    }

    pub fn name(&self) -> &'static str {
        match self {
            ChecksumAlgorithm::Sha256 => "SHA-256",
            ChecksumAlgorithm::Sha512 => "SHA-512",
            ChecksumAlgorithm::Md5 => "MD5",
            ChecksumAlgorithm::Crc32 => "CRC32",
        }
    }

    pub fn from_hex_length(len: usize) -> Option<Self> {
        match len {
            64 => Some(ChecksumAlgorithm::Sha256),
            128 => Some(ChecksumAlgorithm::Sha512),
            32 => Some(ChecksumAlgorithm::Md5),
            8 => Some(ChecksumAlgorithm::Crc32),
            _ => None,
        }
    }
}

impl std::str::FromStr for ChecksumAlgorithm {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match s.as_str() {
            "sha256" | "sha-256" => Ok(ChecksumAlgorithm::Sha256),
            "sha512" | "sha-512" => Ok(ChecksumAlgorithm::Sha512),
            "md5" => Ok(ChecksumAlgorithm::Md5),
            "crc32" | "crc-32" => Ok(ChecksumAlgorithm::Crc32),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChecksumEntry {
    pub checksum: String,
    pub filename: String,
    pub algorithm: Option<ChecksumAlgorithm>,
}

pub fn parse_checksum_file(content: &str) -> Vec<ChecksumEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(entry) = parse_bsd_format(line) {
            entries.push(entry);
            continue;
        }

        if let Some(entry) = parse_gnu_format(line) {
            entries.push(entry);
            continue;
        }
    }

    entries
}

fn parse_bsd_format(line: &str) -> Option<ChecksumEntry> {
    let parts: Vec<&str> = line.splitn(2, " (").collect();
    if parts.len() != 2 {
        return None;
    }

    let algorithm = parts[0].parse::<ChecksumAlgorithm>().ok();

    let rest = parts[1];
    let parts: Vec<&str> = rest.splitn(2, ") = ").collect();
    if parts.len() != 2 {
        return None;
    }

    let filename = parts[0].to_string();
    let checksum = parts[1].trim().to_lowercase();

    if checksum.is_empty() || filename.is_empty() {
        return None;
    }

    Some(ChecksumEntry {
        checksum,
        filename,
        algorithm,
    })
}

fn parse_gnu_format(line: &str) -> Option<ChecksumEntry> {
    let mut split_idx = None;
    let chars: Vec<char> = line.chars().collect();

    for i in 0..chars.len() {
        if chars[i] == ' ' {
            if i + 1 < chars.len() && (chars[i + 1] == ' ' || chars[i + 1] == '*') {
                split_idx = Some(i);
                break;
            }
        }
    }

    let idx = split_idx?;
    let checksum = line[..idx].trim().to_lowercase();
    let mut filename = line[idx..].trim();

    if filename.starts_with('*') {
        filename = &filename[1..];
    }

    if checksum.is_empty() || filename.is_empty() {
        return None;
    }

    let algorithm = ChecksumAlgorithm::from_hex_length(checksum.len());

    Some(ChecksumEntry {
        checksum,
        filename: filename.to_string(),
        algorithm,
    })
}

pub fn find_checksum_for_file<'a>(
    entries: &'a [ChecksumEntry],
    filename: &str,
) -> Option<&'a ChecksumEntry> {
    if let Some(entry) = entries.iter().find(|e| e.filename == filename) {
        return Some(entry);
    }

    let base_name = std::path::Path::new(filename)
        .file_name()
        .and_then(|s| s.to_str())?;

    entries.iter().find(|e| {
        let entry_base = std::path::Path::new(&e.filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&e.filename);
        entry_base == base_name
    })
}
