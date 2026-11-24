use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

use crate::source::SourceType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LengthHint {
    Exact(u64),
    AtLeast(u64),
}

pub(crate) fn read(path: &str, source_type: SourceType) -> Option<LengthHint> {
    let mut file = File::open(path).ok()?;
    match source_type {
        SourceType::Gzip => gzip(&mut file),
        SourceType::Xz => {
            let data_end = data_end(&file);
            xz(&mut file, data_end)
        }
        SourceType::Zstd => zstd(&mut file),
        _ => None,
    }
}

const GZIP_HEADER_AND_TRAILER_LEN: u64 = 10 + 8;

/// A gzip file ends with ISIZE, its last member's length mod 2^32. That is
/// at most the member's length, and so at most the image's.
///
/// Engraver accepts zero padding after the last member. With `k` bytes of
/// padding, the last four bytes hold ISIZE shifted right by `8 * k` bits,
/// or zero once `k` reaches 4, so padding only lowers the bound.
fn gzip<R: Read + Seek>(file: &mut R) -> Option<LengthHint> {
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).ok()?;
    let len = file.seek(SeekFrom::End(0)).ok()?;
    if magic != [0x1f, 0x8b] || len < GZIP_HEADER_AND_TRAILER_LEN {
        return None;
    }
    let isize = read_array::<_, 4>(file, len - 4)?;
    Some(LengthHint::AtLeast(u32::from_le_bytes(isize).into()))
}

/// The first frame's content size, when its header records one. The decoder
/// checks a frame against it, and the image holds at least the first frame.
fn zstd<R: Read>(file: &mut R) -> Option<LengthHint> {
    // ZSTD_FRAMEHEADERSIZE_MAX
    const FRAME_HEADER_MAX_LEN: u64 = 18;

    let mut header = Vec::new();
    file.take(FRAME_HEADER_MAX_LEN)
        .read_to_end(&mut header)
        .ok()?;
    let size = zstd::zstd_safe::get_frame_content_size(&header).ok()??;
    Some(LengthHint::AtLeast(size))
}

const XZ_HEADER_MAGIC: [u8; 6] = [0xfd, b'7', b'z', b'X', b'Z', 0x00];
const XZ_FOOTER_MAGIC: [u8; 2] = *b"YZ";
const XZ_HEADER_LEN: u64 = 12;
const XZ_FOOTER_LEN: u64 = 12;

/// An xz file is one or more streams, each followed by zero padding in
/// multiples of four bytes. Each stream's index records its blocks'
/// uncompressed size, and liblzma checks the blocks against it.
///
/// `data_end` is where the file's last data region ends, if the filesystem
/// says. The bytes after it are a hole, so they are zeros.
fn xz<R: Read + Seek>(file: &mut R, data_end: Option<u64>) -> Option<LengthHint> {
    let mut end = file.seek(SeekFrom::End(0)).ok()?;
    let mut zeros_from = data_end.unwrap_or(end);
    let mut total = None;
    loop {
        let padding = trailing_zeros(file, end, zeros_from)?;
        let stream = if padding % 4 == 0 {
            xz_stream(file, end - padding)
        } else {
            None
        };
        let Some(Stream {
            start,
            uncompressed_len,
        }) = stream
        else {
            return total.map(LengthHint::AtLeast);
        };

        let sum = uncompressed_len.checked_add(total.unwrap_or(0))?;
        if start == 0 {
            return Some(LengthHint::Exact(sum));
        }
        total = Some(sum);
        end = start;
        zeros_from = end;
    }
}

struct Stream {
    start: u64,
    uncompressed_len: u64,
}

fn xz_stream<R: Read + Seek>(file: &mut R, end: u64) -> Option<Stream> {
    let footer_start = end.checked_sub(XZ_FOOTER_LEN)?;
    let footer = read_array::<_, 12>(file, footer_start)?;
    let (crc, rest) = footer.split_at(4);
    let (backward_size, rest) = rest.split_at(4);
    let (flags, magic) = rest.split_at(2);
    if magic != XZ_FOOTER_MAGIC
        || !crc_matches(&footer[4..10], crc)
        || flags[0] != 0
        || flags[1] & 0xf0 != 0
    {
        return None;
    }

    let backward_size = u32::from_le_bytes(backward_size.try_into().ok()?);
    let index_len = (u64::from(backward_size) + 1) * 4;
    let index_start = footer_start.checked_sub(index_len)?;
    file.seek(SeekFrom::Start(index_start)).ok()?;
    let index = Index::parse(BufReader::new(file.by_ref().take(index_len)), index_len)?;

    let header_start = index_start
        .checked_sub(index.blocks_len)?
        .checked_sub(XZ_HEADER_LEN)?;
    let header = read_array::<_, 12>(file, header_start)?;
    if header[..6] != XZ_HEADER_MAGIC
        || header[6..8] != *flags
        || !crc_matches(&header[6..8], &header[8..])
    {
        return None;
    }
    Some(Stream {
        start: header_start,
        uncompressed_len: index.uncompressed_len,
    })
}

struct Index {
    blocks_len: u64,
    uncompressed_len: u64,
}

impl Index {
    fn parse(index: impl Read, len: u64) -> Option<Self> {
        // liblzma's UNPADDED_SIZE_MIN
        const MIN_UNPADDED_LEN: u64 = 5;

        let mut index = Crc32Reader {
            inner: index,
            crc: crc32fast::Hasher::new(),
            len: 0,
        };
        if index.byte()? != 0 {
            return None;
        }
        let records = index.varint()?;
        let mut blocks_len = 0u64;
        let mut uncompressed_len = 0u64;
        for _ in 0..records {
            let unpadded_len = index.varint()?;
            let block_uncompressed_len = index.varint()?;
            if unpadded_len < MIN_UNPADDED_LEN {
                return None;
            }
            blocks_len = blocks_len.checked_add(unpadded_len.checked_next_multiple_of(4)?)?;
            uncompressed_len = uncompressed_len.checked_add(block_uncompressed_len)?;
        }
        while index.len % 4 != 0 {
            if index.byte()? != 0 {
                return None;
            }
        }

        let crc = index.crc.finalize();
        let mut stored = [0u8; 4];
        index.inner.read_exact(&mut stored).ok()?;
        (crc == u32::from_le_bytes(stored) && index.len + 4 == len).then_some(Self {
            blocks_len,
            uncompressed_len,
        })
    }
}

struct Crc32Reader<R> {
    inner: R,
    crc: crc32fast::Hasher,
    len: u64,
}

impl<R: Read> Crc32Reader<R> {
    fn byte(&mut self) -> Option<u8> {
        let mut byte = [0u8];
        self.inner.read_exact(&mut byte).ok()?;
        self.crc.update(&byte);
        self.len += 1;
        Some(byte[0])
    }

    /// xz forbids a multibyte integer whose last byte is zero, except a lone
    /// zero (xz file format, section 1.2)
    fn varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for i in 0..9 {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return (i == 0 || byte != 0).then_some(value);
            }
        }
        None
    }
}

fn crc_matches(data: &[u8], stored: &[u8]) -> bool {
    <[u8; 4]>::try_from(stored).is_ok_and(|s| u32::from_le_bytes(s) == crc32fast::hash(data))
}

/// The length of the zero run that ends at `end`. The bytes from `zeros_from`
/// to `end` are known to be zeros, so the scan starts at `zeros_from`.
fn trailing_zeros<R: Read + Seek>(file: &mut R, end: u64, zeros_from: u64) -> Option<u64> {
    let mut buf = [0u8; 4096];
    let mut pos = zeros_from.min(end);
    while pos > 0 {
        let n = pos.min(buf.len() as u64);
        let chunk = &mut buf[..n as usize];
        file.seek(SeekFrom::Start(pos - n)).ok()?;
        file.read_exact(chunk).ok()?;
        let zeros = chunk.iter().rev().take_while(|&&b| b == 0).count() as u64;
        pos -= zeros;
        if zeros < n {
            break;
        }
    }
    Some(end - pos)
}

/// Where the file's last data region ends. A file with holes, such as one
/// made by `truncate`, ends in a hole whose zeros the filesystem never
/// stores, so reading them back is wasted work. `None` when the filesystem
/// or the platform cannot say, and the caller then scans the whole tail.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd"
))]
fn data_end(file: &File) -> Option<u64> {
    use std::os::fd::AsRawFd;

    let fd = file.as_raw_fd();
    let seek = |offset: libc::off_t, whence| {
        // SAFETY: lseek only moves the offset of a descriptor that `file`
        // keeps open for the whole call.
        #[allow(unsafe_code)]
        let at = unsafe { libc::lseek(fd, offset, whence) };
        (at >= 0).then_some(at)
    };

    let mut end = 0;
    loop {
        let Some(data) = seek(end, libc::SEEK_DATA) else {
            // ENXIO means no data at or after `end`, and anything else
            // means the filesystem cannot tell.
            let no_more_data = std::io::Error::last_os_error().raw_os_error() == Some(libc::ENXIO);
            return no_more_data.then(|| u64::try_from(end).ok()).flatten();
        };
        end = seek(data, libc::SEEK_HOLE).filter(|&hole| hole > data)?;
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd"
)))]
fn data_end(_: &File) -> Option<u64> {
    None
}

fn read_array<R: Read + Seek, const N: usize>(file: &mut R, offset: u64) -> Option<[u8; N]> {
    let mut buf = [0u8; N];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}
