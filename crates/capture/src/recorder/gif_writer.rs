use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use deckhand_core::{Context, Error, Result, protocol::Quality};
use gif::{DisposalMethod, Encoder, Frame, Repeat};

use super::bitrate::gif_speed;
use super::changed_area::changed_area;
use super::clock::gif_delay;
use super::layout::{Placement, Size};

const BYTES_PER_PIXEL: usize = 4;

struct Pending {
    frame: Frame<'static>,
    first_tick: u64,
}

pub(crate) struct GifWriter {
    encoder: Encoder<BufWriter<File>>,
    size: Size,
    fps: u32,
    speed: i32,
    shown: Vec<u8>,
    rgba: Vec<u8>,
    pending: Option<Pending>,
}

impl GifWriter {
    pub(crate) fn create(path: &Path, size: Size, fps: u32, quality: Quality) -> Result<Self> {
        let too_large = || Error::invalid("GIF frames are limited to 65535 px per side");
        let width = u16::try_from(size.width).map_err(|_| too_large())?;
        let height = u16::try_from(size.height).map_err(|_| too_large())?;
        let file = File::create(path).context("creating the GIF file")?;
        let mut encoder = Encoder::new(BufWriter::new(file), width, height, &[]).context("writing the GIF header")?;
        encoder.set_repeat(Repeat::Infinite).context("making the GIF loop")?;
        Ok(Self { encoder, size, fps, speed: gif_speed(quality), shown: Vec::new(), rgba: Vec::new(), pending: None })
    }

    pub(crate) fn write(&mut self, bgra: &[u8], tick: u64) -> Result<()> {
        if bgra.len() != self.size.bgra_len() {
            return Err(Error::internal("the GIF frame does not match the video size"));
        }
        let area = if self.shown.is_empty() { Some(self.full()) } else { changed_area(&self.shown, bgra, self.size) };
        let Some(area) = area else { return Ok(()) };
        self.flush_pending(tick)?;
        let frame = self.quantize(bgra, area)?;
        self.pending = Some(Pending { frame, first_tick: tick });
        self.shown.clear();
        self.shown.extend_from_slice(bgra);
        Ok(())
    }

    pub(crate) fn finish(mut self, end_tick: u64) -> Result<()> {
        self.flush_pending(end_tick)?;
        let mut file = self.encoder.into_inner().context("finishing the GIF file")?;
        file.flush().context("writing the GIF file")
    }

    const fn full(&self) -> Placement {
        Placement { x: 0, y: 0, width: self.size.width, height: self.size.height }
    }

    fn flush_pending(&mut self, until_tick: u64) -> Result<()> {
        let Some(Pending { mut frame, first_tick }) = self.pending.take() else { return Ok(()) };
        frame.delay = gif_delay(first_tick, until_tick, self.fps).max(1);
        self.encoder.write_frame(&frame).context("writing a GIF frame")
    }

    fn quantize(&mut self, bgra: &[u8], area: Placement) -> Result<Frame<'static>> {
        let too_large = || Error::internal("the GIF frame area is too large");
        let width = u16::try_from(area.width).map_err(|_| too_large())?;
        let height = u16::try_from(area.height).map_err(|_| too_large())?;
        copy_as_rgba(bgra, self.size, area, &mut self.rgba);
        if self.rgba.len() != usize::from(width) * usize::from(height) * BYTES_PER_PIXEL {
            return Err(Error::internal("the GIF frame area lies outside the video"));
        }
        let mut frame = Frame::from_rgba_speed(width, height, &mut self.rgba, self.speed.clamp(1, 30));
        frame.left = u16::try_from(area.x).map_err(|_| too_large())?;
        frame.top = u16::try_from(area.y).map_err(|_| too_large())?;
        frame.dispose = DisposalMethod::Keep;
        Ok(frame)
    }
}

fn copy_as_rgba(bgra: &[u8], size: Size, area: Placement, rgba: &mut Vec<u8>) {
    let stride = size.width as usize * BYTES_PER_PIXEL;
    let (left, width) = (area.x as usize * BYTES_PER_PIXEL, area.width as usize * BYTES_PER_PIXEL);
    rgba.clear();
    rgba.reserve(area.width as usize * area.height as usize * BYTES_PER_PIXEL);
    for row in area.y as usize..area.y as usize + area.height as usize {
        let start = row * stride + left;
        let Some(line) = bgra.get(start..start + width) else { return };
        for pixel in line.as_chunks::<BYTES_PER_PIXEL>().0 {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_copies_swap_channels_inside_the_area_only() {
        let size = Size::new(3, 2);
        let bgra: Vec<u8> = (0..24).collect();
        let mut rgba = Vec::new();
        copy_as_rgba(&bgra, size, Placement { x: 1, y: 1, width: 2, height: 1 }, &mut rgba);
        assert_eq!(rgba, [18, 17, 16, 255, 22, 21, 20, 255]);
    }
}
