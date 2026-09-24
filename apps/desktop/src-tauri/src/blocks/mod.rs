use crate::stream_seal::{block_size, first_byte_size};

#[inline]
pub fn block_size_bytes() -> u64 {
    block_size()
}

#[inline]
pub fn first_byte_bytes() -> u64 {
    first_byte_size()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSpan {
    pub index: u64,
    pub offset_in_block: u64,
    pub length: u64,
}

pub fn blocks_for_range(offset: u64, length: u64) -> Vec<BlockSpan> {
    if length == 0 {
        return Vec::new();
    }
    let bs = block_size();
    let mut out = Vec::new();
    let mut remaining = length;
    let mut cur = offset;
    while remaining > 0 {
        let index = cur / bs;
        let offset_in_block = cur % bs;
        let take = (bs - offset_in_block).min(remaining);
        out.push(BlockSpan {
            index,
            offset_in_block,
            length: take,
        });
        cur += take;
        remaining -= take;
    }
    out
}

pub fn block_byte_range(index: u64, file_size: u64) -> (u64, u64) {
    let bs = block_size();
    let start = index * bs;
    if start >= file_size {
        return (start, 0);
    }
    let end = ((index + 1) * bs).min(file_size);
    (start, end - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_single_block() {
        let spans = blocks_for_range(100, 50);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset_in_block, 100);
        assert_eq!(spans[0].length, 50);
    }

    #[test]
    fn spans_cross_boundary() {
        let bs = block_size_bytes();
        let spans = blocks_for_range(bs - 10, 20);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].length, 10);
        assert_eq!(spans[1].index, 1);
        assert_eq!(spans[1].offset_in_block, 0);
        assert_eq!(spans[1].length, 10);
    }
}
