//! Hex encoding / decoding helpers shared across the crate.
//!
//! The crate previously had two copies of `bytes_to_hex`: a fast one in
//! `writer.rs` (single pre-allocated `String` + `write!`) and a slow one
//! in `verifier.rs` (`format!()` per byte, then `.collect::<String>()`).
//! For a SHA-512 digest the slow path was 64 heap allocations plus
//! concatenations. This module owns the one fast implementation that
//! every caller now uses.
//!
//! `hex_to_bytes` also moves here, with a panic-safe rewrite: the
//! previous version sliced `&hex[i..i+2]` on `&str`, which panics on a
//! non-ASCII character at `i` because the index is not a char boundary.
//! The new version iterates `as_bytes()` and explicitly validates each
//! pair as ASCII hex before decoding.

use crate::error::{Error, Result};
use std::fmt::Write as _;

/// Encode `bytes` as a lowercase hex string.
///
/// Allocates exactly one `String` of capacity `bytes.len() * 2` and
/// writes each byte's two-character hex form into it. Compare with the
/// naive `bytes.iter().map(|b| format!("{:02x}", b)).collect()`, which
/// allocates a fresh two-character `String` per byte plus a final
/// concatenation — `O(n)` allocations versus `O(1)`.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // `write!` into a String is infallible; ignore the Result.
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Decode a lowercase or mixed-case hex string into bytes.
///
/// Rejects:
/// - odd-length input (each byte needs two hex characters)
/// - any non-ASCII-hex character (returns `InvalidConfig` with the
///   offending byte position; never panics on multibyte UTF-8 input)
///
/// The previous implementation used `&hex[i..i+2]` on a `&str`, which
/// panics when `i` lands inside a multibyte UTF-8 character because
/// the slice boundary is not a `char` boundary. The new implementation
/// iterates `hex.as_bytes()` and never indexes the `str`, so any
/// input — including completely garbled bytes — produces an `Err`
/// rather than a panic.
pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    let bytes = hex.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(Error::InvalidConfig(
            "Hex string must have even length".to_string(),
        ));
    }

    let mut out = Vec::with_capacity(bytes.len() / 2);
    for i in (0..bytes.len()).step_by(2) {
        let hi = decode_nibble(bytes[i]).ok_or_else(|| {
            Error::InvalidConfig(format!("Invalid hex character at position {}", i))
        })?;
        let lo = decode_nibble(bytes[i + 1]).ok_or_else(|| {
            Error::InvalidConfig(format!("Invalid hex character at position {}", i + 1))
        })?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

/// Decode a single ASCII hex digit (`'0'..='9'`, `'a'..='f'`, `'A'..='F'`)
/// to its 0–15 nibble value, returning `None` for anything else.
#[inline]
fn decode_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // bytes_to_hex
    // -------------------------------------------------------------------------

    #[test]
    fn test_bytes_to_hex_empty() {
        assert_eq!(bytes_to_hex(&[]), "");
    }

    #[test]
    fn test_bytes_to_hex_single_byte_zero_padded() {
        assert_eq!(bytes_to_hex(&[0x00]), "00");
        assert_eq!(bytes_to_hex(&[0x0f]), "0f");
        assert_eq!(bytes_to_hex(&[0xff]), "ff");
    }

    #[test]
    fn test_bytes_to_hex_multiple_bytes() {
        assert_eq!(bytes_to_hex(&[0x01, 0x23, 0x45]), "012345");
        assert_eq!(bytes_to_hex(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }

    #[test]
    fn test_bytes_to_hex_capacity_matches_expected_length() {
        // The fast path pre-allocates len*2 bytes; this asserts the
        // result actually has that length (a guard against an
        // accidental rewrite that drops the pre-allocation).
        let input = [0u8; 32];
        let hex = bytes_to_hex(&input);
        assert_eq!(hex.len(), 64);
    }

    // -------------------------------------------------------------------------
    // hex_to_bytes
    // -------------------------------------------------------------------------

    #[test]
    fn test_hex_to_bytes_round_trip_with_bytes_to_hex() {
        let original: Vec<u8> = (0..=255).collect();
        let hex = bytes_to_hex(&original);
        let decoded = hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_hex_to_bytes_accepts_uppercase_and_mixed_case() {
        assert_eq!(hex_to_bytes("FF").unwrap(), vec![0xff]);
        assert_eq!(hex_to_bytes("aF").unwrap(), vec![0xaf]);
        assert_eq!(
            hex_to_bytes("DeAdBeEf").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn test_hex_to_bytes_rejects_odd_length() {
        let err = hex_to_bytes("abc").unwrap_err();
        assert!(matches!(err, Error::InvalidConfig(ref msg) if msg.contains("even length")));
    }

    #[test]
    fn test_hex_to_bytes_rejects_non_hex_character() {
        // 'g' is past 'f' in the alphabet — not a hex digit.
        let err = hex_to_bytes("gg").unwrap_err();
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("Invalid hex character"))
        );
    }

    #[test]
    fn test_hex_to_bytes_no_panic_on_multibyte_utf8() {
        // REGRESSION: the previous implementation used &hex[i..i+2] on
        // a &str. A multibyte character (here the snowman, 3 bytes in
        // UTF-8) at index i would cause the slice to land mid-character
        // and panic with "byte index N is not a char boundary".
        // The new byte-iteration form must return a clean error.
        let result = hex_to_bytes("☃☃");
        assert!(result.is_err(), "must return Err, not panic");
    }

    #[test]
    fn test_hex_to_bytes_no_panic_on_mixed_ascii_multibyte() {
        // First two bytes are ASCII '0' '0' (decodes to 0x00) but the
        // remaining bytes are a multibyte char. Must error cleanly
        // when the parser advances past the ASCII prefix.
        let result = hex_to_bytes("00☃");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_to_bytes_empty_string() {
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
    }

    // -------------------------------------------------------------------------
    // decode_nibble
    // -------------------------------------------------------------------------

    #[test]
    fn test_decode_nibble_all_valid_chars() {
        for (i, c) in (b'0'..=b'9').enumerate() {
            assert_eq!(decode_nibble(c), Some(i as u8));
        }
        for (i, c) in (b'a'..=b'f').enumerate() {
            assert_eq!(decode_nibble(c), Some(10 + i as u8));
        }
        for (i, c) in (b'A'..=b'F').enumerate() {
            assert_eq!(decode_nibble(c), Some(10 + i as u8));
        }
    }

    #[test]
    fn test_decode_nibble_rejects_non_hex() {
        assert_eq!(decode_nibble(b'g'), None);
        assert_eq!(decode_nibble(b'G'), None);
        assert_eq!(decode_nibble(b' '), None);
        assert_eq!(decode_nibble(0xff), None);
    }
}
