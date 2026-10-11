use deckhand_core::{Error, Result};

pub(crate) const BYTES_PER_PIXEL: usize = 4;

pub(crate) struct Pixels<'a> {
    pub bytes: &'a [u8],
    pub stride: usize,
}

pub(crate) struct PixelsMut<'a> {
    pub bytes: &'a mut [u8],
    pub stride: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Block {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

pub(crate) fn copy_block(source: &Pixels, from: Block, target: &mut PixelsMut, to: (usize, usize)) -> Result<()> {
    let row_bytes = from.width * BYTES_PER_PIXEL;
    for row in 0..from.height {
        let source_start = (from.y + row) * source.stride + from.x * BYTES_PER_PIXEL;
        let target_start = (to.1 + row) * target.stride + to.0 * BYTES_PER_PIXEL;
        let source_row = source.bytes.get(source_start..source_start + row_bytes);
        let target_row = target.bytes.get_mut(target_start..target_start + row_bytes);
        let (Some(source_row), Some(target_row)) = (source_row, target_row) else {
            return Err(Error::internal("pixel copy is out of bounds"));
        };
        target_row.copy_from_slice(source_row);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(width: usize, height: usize) -> Vec<u8> {
        (0..width * height * BYTES_PER_PIXEL).map(|value| value as u8).collect()
    }

    #[test]
    fn copies_a_block_between_strides() {
        let source = numbered(4, 3);
        let mut target = vec![0u8; 3 * 2 * BYTES_PER_PIXEL];
        let block = Block { x: 1, y: 1, width: 2, height: 2 };
        let mut view = PixelsMut { bytes: &mut target, stride: 3 * BYTES_PER_PIXEL };
        copy_block(&Pixels { bytes: &source, stride: 16 }, block, &mut view, (1, 0)).unwrap();
        assert_eq!(&target[4..12], &source[20..28]);
        assert_eq!(&target[16..24], &source[36..44]);
        assert_eq!(&target[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn rejects_out_of_bounds_copies() {
        let source = numbered(2, 2);
        let mut target = vec![0u8; 2 * 2 * BYTES_PER_PIXEL];
        let block = Block { x: 1, y: 1, width: 2, height: 2 };
        let mut view = PixelsMut { bytes: &mut target, stride: 8 };
        assert!(copy_block(&Pixels { bytes: &source, stride: 8 }, block, &mut view, (0, 0)).is_err());
    }
}
