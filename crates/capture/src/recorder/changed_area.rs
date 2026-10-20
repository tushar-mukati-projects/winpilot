use super::layout::Size;
use crate::pixels::{BYTES_PER_PIXEL, Block};

pub(crate) fn changed_area(before: &[u8], after: &[u8], size: Size) -> Option<Block> {
    let stride = size.width as usize * BYTES_PER_PIXEL;
    if stride == 0 || before.len() != after.len() || after.len() != size.bgra_len() {
        return Some(Block { x: 0, y: 0, width: size.width, height: size.height });
    }
    let rows_differ = |row: &usize| before[row * stride..(row + 1) * stride] != after[row * stride..(row + 1) * stride];
    let height = size.height as usize;
    let top = (0..height).find(rows_differ)?;
    let bottom = (top..height).rev().find(rows_differ).unwrap_or(top);
    let (left, right) = changed_columns(before, after, stride, top..=bottom);
    Some(Block { x: left as u32, y: top as u32, width: (right - left + 1) as u32, height: (bottom - top + 1) as u32 })
}

fn changed_columns(
    before: &[u8],
    after: &[u8],
    stride: usize,
    rows: std::ops::RangeInclusive<usize>,
) -> (usize, usize) {
    let width = stride / BYTES_PER_PIXEL;
    let (mut left, mut right) = (width - 1, 0);
    for row in rows {
        let old = before[row * stride..(row + 1) * stride].as_chunks::<BYTES_PER_PIXEL>().0;
        let new = after[row * stride..(row + 1) * stride].as_chunks::<BYTES_PER_PIXEL>().0;
        if let Some(first) = (0..left).find(|&column| old[column] != new[column]) {
            left = first;
        }
        if let Some(last) = (right + 1..width).rev().find(|&column| old[column] != new[column]) {
            right = last;
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(size: Size) -> Vec<u8> {
        vec![0; size.bgra_len()]
    }

    fn paint(buffer: &mut [u8], size: Size, x: u32, y: u32) {
        let start = (y as usize * size.width as usize + x as usize) * 4;
        buffer[start..start + 4].copy_from_slice(&[9, 9, 9, 255]);
    }

    #[test]
    fn identical_frames_have_no_changed_area() {
        let size = Size::new(8, 6);
        assert_eq!(changed_area(&canvas(size), &canvas(size), size), None);
    }

    #[test]
    fn a_single_pixel_change_is_found_exactly() {
        let size = Size::new(8, 6);
        let mut after = canvas(size);
        paint(&mut after, size, 5, 3);
        assert_eq!(changed_area(&canvas(size), &after, size), Some(Block { x: 5, y: 3, width: 1, height: 1 }));
    }

    #[test]
    fn scattered_changes_are_boxed_together() {
        let size = Size::new(10, 10);
        let mut after = canvas(size);
        paint(&mut after, size, 7, 1);
        paint(&mut after, size, 2, 4);
        paint(&mut after, size, 4, 8);
        assert_eq!(changed_area(&canvas(size), &after, size), Some(Block { x: 2, y: 1, width: 6, height: 8 }));
    }

    #[test]
    fn edges_and_corners_are_included() {
        let size = Size::new(4, 3);
        let mut after = canvas(size);
        paint(&mut after, size, 0, 0);
        paint(&mut after, size, 3, 2);
        assert_eq!(changed_area(&canvas(size), &after, size), Some(Block { x: 0, y: 0, width: 4, height: 3 }));
        let mut right_edge = canvas(size);
        paint(&mut right_edge, size, 3, 1);
        assert_eq!(changed_area(&canvas(size), &right_edge, size), Some(Block { x: 3, y: 1, width: 1, height: 1 }));
    }

    #[test]
    fn mismatched_buffers_count_as_a_full_change() {
        let size = Size::new(4, 4);
        let full = Some(Block { x: 0, y: 0, width: 4, height: 4 });
        assert_eq!(changed_area(&[0; 8], &canvas(size), size), full);
        assert_eq!(changed_area(&[], &[], Size::new(0, 4)), Some(Block { x: 0, y: 0, width: 0, height: 4 }));
    }
}
