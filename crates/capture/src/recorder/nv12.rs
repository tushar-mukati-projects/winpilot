use deckhand_core::{Error, Result};

use super::layout::Size;
use crate::pixels::BYTES_PER_PIXEL;

pub(crate) const fn nv12_len(size: Size) -> usize {
    size.pixels() + size.pixels() / 2
}

pub(crate) fn bgra_to_nv12(bgra: &[u8], size: Size, nv12: &mut [u8]) -> Result<()> {
    let (width, height) = (size.width as usize, size.height as usize);
    if !size.width.is_multiple_of(2)
        || !size.height.is_multiple_of(2)
        || bgra.len() != size.bgra_len()
        || nv12.len() != nv12_len(size)
    {
        return Err(Error::internal("NV12 conversion needs even sides and matching buffer sizes"));
    }
    let (luma, chroma) = nv12.split_at_mut(width * height);
    for (pair, (luma_rows, chroma_row)) in
        luma.chunks_exact_mut(width * 2).zip(chroma.chunks_exact_mut(width)).enumerate()
    {
        let source = &bgra[pair * 2 * width * BYTES_PER_PIXEL..(pair * 2 + 2) * width * BYTES_PER_PIXEL];
        convert_row_pair(source, width, luma_rows, chroma_row);
    }
    Ok(())
}

fn convert_row_pair(source: &[u8], width: usize, luma_rows: &mut [u8], chroma_row: &mut [u8]) {
    let (top, bottom) = source.split_at(width * BYTES_PER_PIXEL);
    let (top_luma, bottom_luma) = luma_rows.split_at_mut(width);
    let top_pixels = top.as_chunks::<BYTES_PER_PIXEL>().0;
    let bottom_pixels = bottom.as_chunks::<BYTES_PER_PIXEL>().0;
    for (column, uv) in chroma_row.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let left = column * 2;
        let block = [top_pixels[left], top_pixels[left + 1], bottom_pixels[left], bottom_pixels[left + 1]];
        top_luma[left] = luma(block[0]);
        top_luma[left + 1] = luma(block[1]);
        bottom_luma[left] = luma(block[2]);
        bottom_luma[left + 1] = luma(block[3]);
        *uv = chroma(block);
    }
}

fn luma([blue, green, red, _]: [u8; 4]) -> u8 {
    let weighted = 47 * i32::from(red) + 157 * i32::from(green) + 16 * i32::from(blue);
    (16 + ((weighted + 128) >> 8)) as u8
}

fn chroma(block: [[u8; 4]; 4]) -> [u8; 2] {
    let sum = |channel: usize| block.iter().map(|pixel| i32::from(pixel[channel])).sum::<i32>();
    let (blue, green, red) = (sum(0), sum(1), sum(2));
    let blue_difference = (-26 * red - 86 * green + 112 * blue + 512) >> 10;
    let red_difference = (112 * red - 102 * green - 10 * blue + 512) >> 10;
    [(128 + blue_difference).clamp(16, 240) as u8, (128 + red_difference).clamp(16, 240) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: Size, [blue, green, red]: [u8; 3]) -> Vec<u8> {
        [blue, green, red, 255].repeat(size.pixels())
    }

    fn converted(bgra: &[u8], size: Size) -> Vec<u8> {
        let mut nv12 = vec![0; nv12_len(size)];
        bgra_to_nv12(bgra, size, &mut nv12).unwrap();
        nv12
    }

    fn assert_solid(nv12: &[u8], size: Size, expected: [u8; 3]) {
        let (luma, chroma) = nv12.split_at(size.pixels());
        assert!(luma.iter().all(|&value| value == expected[0]), "luma {:?}", &luma[..4]);
        assert!(
            chroma.as_chunks::<2>().0.iter().all(|uv| *uv == [expected[1], expected[2]]),
            "chroma {:?}",
            &chroma[..2]
        );
    }

    #[test]
    fn primaries_land_on_bt709_limited_range_values() {
        let size = Size::new(4, 2);
        assert_solid(&converted(&solid(size, [0, 0, 0]), size), size, [16, 128, 128]);
        assert_solid(&converted(&solid(size, [255, 255, 255]), size), size, [235, 128, 128]);
        assert_solid(&converted(&solid(size, [0, 0, 255]), size), size, [63, 102, 240]);
        assert_solid(&converted(&solid(size, [0, 255, 0]), size), size, [172, 42, 26]);
        assert_solid(&converted(&solid(size, [255, 0, 0]), size), size, [32, 240, 118]);
    }

    #[test]
    fn chroma_averages_each_two_by_two_block() {
        let size = Size::new(2, 2);
        let mut bgra = solid(size, [0, 0, 0]);
        bgra[..4].copy_from_slice(&[255, 255, 255, 255]);
        bgra[4..8].copy_from_slice(&[255, 255, 255, 255]);
        let nv12 = converted(&bgra, size);
        assert_eq!(&nv12[..4], &[235, 235, 16, 16]);
        assert_eq!(&nv12[4..], &[128, 128]);
    }

    #[test]
    fn layout_is_luma_rows_then_interleaved_chroma_rows() {
        let size = Size::new(4, 4);
        let mut bgra = solid(size, [0, 0, 0]);
        for pixel in &mut bgra.as_chunks_mut::<4>().0[8..] {
            *pixel = [0, 0, 255, 255];
        }
        let nv12 = converted(&bgra, size);
        assert_eq!(&nv12[..8], &[16; 8]);
        assert_eq!(&nv12[8..16], &[63; 8]);
        assert_eq!(&nv12[16..20], &[128, 128, 128, 128]);
        assert_eq!(&nv12[20..24], &[102, 240, 102, 240]);
    }

    #[test]
    fn odd_sizes_and_short_buffers_are_rejected_without_panicking() {
        let size = Size::new(3, 2);
        let mut nv12 = vec![0; 9];
        assert!(bgra_to_nv12(&solid(size, [1, 2, 3]), size, &mut nv12).is_err());
        let even = Size::new(2, 2);
        let mut short = vec![0; 3];
        assert!(bgra_to_nv12(&solid(even, [1, 2, 3]), even, &mut short).is_err());
        let mut fine = vec![0; 6];
        assert!(bgra_to_nv12(&[0; 12], even, &mut fine).is_err());
    }

    #[test]
    fn conversion_of_a_full_frame_is_timed() {
        let size = Size::new(1920, 1200);
        let bgra: Vec<u8> = (0..size.bgra_len()).map(|index| (index * 7 % 251) as u8).collect();
        let mut nv12 = vec![0; nv12_len(size)];
        let started = std::time::Instant::now();
        for _ in 0..5 {
            bgra_to_nv12(&bgra, size, &mut nv12).unwrap();
        }
        let average = started.elapsed().as_secs_f64() * 1000.0 / 5.0;
        eprintln!("timing: BGRA to NV12 at 1920x1200: {average:.2} ms per frame");
    }
}
