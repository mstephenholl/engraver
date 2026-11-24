//! `O_DIRECT` on Linux and `FILE_FLAG_NO_BUFFERING` on Windows move whole
//! blocks only, from a block-aligned buffer address at a block-aligned file
//! offset.

use crate::{align_up, is_aligned};

pub(crate) struct AlignedBuffer {
    data: Vec<u8>,
    alignment: usize,
}

impl AlignedBuffer {
    pub(crate) fn new(size: usize, alignment: usize) -> Self {
        let total_size = size + alignment;
        let data = vec![0u8; total_size];
        Self { data, alignment }
    }

    pub(crate) fn as_aligned_slice_mut(&mut self, len: usize) -> &mut [u8] {
        if self.data.len() < len + self.alignment {
            self.data = vec![0u8; len + self.alignment];
        }
        let ptr = self.data.as_ptr();
        let aligned_ptr = align_up(ptr as usize, self.alignment) as *mut u8;
        let offset = aligned_ptr as usize - ptr as usize;
        &mut self.data[offset..offset + len]
    }

    pub(crate) fn read_through(
        &mut self,
        offset: u64,
        buf: &mut [u8],
        read_blocks: impl FnOnce(&mut [u8], u64) -> std::io::Result<usize>,
    ) -> std::io::Result<usize> {
        let alignment = self.alignment;
        let skip = (offset % alignment as u64) as usize;
        if skip == 0
            && is_aligned(buf.as_ptr() as usize, alignment)
            && is_aligned(buf.len(), alignment)
        {
            return read_blocks(buf, offset);
        }

        let blocks = self.as_aligned_slice_mut(align_up(skip + buf.len(), alignment));
        let read = read_blocks(blocks, offset - skip as u64)?;
        let copied = read.saturating_sub(skip).min(buf.len());
        buf[..copied].copy_from_slice(&blocks[skip..skip + copied]);
        Ok(copied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_disk(disk: &[u8], buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
        let aligned = |n: usize| is_aligned(n, 512);
        if !aligned(buf.as_ptr() as usize) || !aligned(buf.len()) || !aligned(offset as usize) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let start = disk.len().min(offset as usize);
        let n = buf.len().min(disk.len() - start);
        buf[..n].copy_from_slice(&disk[start..start + n]);
        Ok(n)
    }

    #[test]
    fn test_read_through_returns_the_bytes_at_any_offset_length_and_address() {
        let disk: Vec<u8> = (0..3 * 512 + 100).map(|i| (i % 251) as u8).collect();
        let mut bounce = AlignedBuffer::new(1024, 512);
        let mut storage = vec![0u8; 4096];
        let base = storage.as_ptr().align_offset(512);

        for offset in [0, 1, 511, 512, 700, 1536, 1600, 1636, 2000] {
            for len in [0, 1, 100, 512, 513, 1024, 1500] {
                for misalignment in [0, 1] {
                    let start = base + misalignment;
                    let buf = &mut storage[start..start + len];
                    let n = bounce
                        .read_through(offset, buf, |blocks, at| read_disk(&disk, blocks, at))
                        .unwrap();

                    let from = disk.len().min(offset as usize);
                    let expected = &disk[from..disk.len().min(from + len)];
                    assert_eq!(&buf[..n], expected, "offset {offset}, len {len}");
                }
            }
        }
    }
}
