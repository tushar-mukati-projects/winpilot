use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use gif::{DisposalMethod, Encoder, Frame, Repeat};
use winpilot_core::{Context, Error, Result, protocol::Quality};

use super::bitrate::gif_speed;
use super::changed_area::changed_area;
use super::clock::gif_delay;
use super::layout::Size;
use crate::pixels::{BYTES_PER_PIXEL, Block, Pixels, PixelsMut, copy_block, rgb_of, u16_sides};

const MAX_FRAME_DELAY: u64 = u16::MAX as u64;
const MAX_TOTAL_DELAY: u64 = 100 * 60 * 60 * 24;
const SLOWEST_QUANTIZER: i32 = 1;
const FASTEST_QUANTIZER: i32 = 30;
const OPAQUE: u8 = 255;

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
        let (width, height) = u16_sides(size.width, size.height, "GIF frames")?;
        let file = OpenOptions::new().write(true).truncate(true).open(path).context("opening the reserved GIF file")?;
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

    const fn full(&self) -> Block {
        Block { x: 0, y: 0, width: self.size.width, height: self.size.height }
    }

    fn flush_pending(&mut self, until_tick: u64) -> Result<()> {
        let Some(Pending { mut frame, first_tick }) = self.pending.take() else { return Ok(()) };
        let mut delays = frame_delays(gif_delay(first_tick, until_tick, self.fps));
        frame.delay = delays.next().unwrap_or(1);
        self.encoder.write_frame(&frame).context("writing a GIF frame")?;
        for delay in delays {
            self.encoder.write_frame(&idle_frame(delay)).context("writing a GIF frame")?;
        }
        Ok(())
    }

    fn quantize(&mut self, bgra: &[u8], area: Block) -> Result<Frame<'static>> {
        let too_large = || Error::internal("the GIF frame area is too large");
        let width = u16::try_from(area.width).map_err(|_| too_large())?;
        let height = u16::try_from(area.height).map_err(|_| too_large())?;
        copy_as_rgba(bgra, self.size, area, &mut self.rgba)?;
        let mut frame = Frame::from_rgba_speed(
            width,
            height,
            &mut self.rgba,
            self.speed.clamp(SLOWEST_QUANTIZER, FASTEST_QUANTIZER),
        );
        frame.left = u16::try_from(area.x).map_err(|_| too_large())?;
        frame.top = u16::try_from(area.y).map_err(|_| too_large())?;
        frame.dispose = DisposalMethod::Keep;
        Ok(frame)
    }
}

fn frame_delays(total: u64) -> impl Iterator<Item = u16> {
    let mut left = total.clamp(1, MAX_TOTAL_DELAY);
    std::iter::from_fn(move || {
        let chunk = left.min(MAX_FRAME_DELAY);
        left -= chunk;
        (chunk > 0).then_some(chunk as u16)
    })
}

fn idle_frame(delay: u16) -> Frame<'static> {
    Frame {
        delay,
        width: 1,
        height: 1,
        transparent: Some(0),
        palette: Some(vec![0, 0, 0]),
        buffer: Cow::Owned(vec![0]),
        dispose: DisposalMethod::Keep,
        ..Frame::default()
    }
}

fn copy_as_rgba(bgra: &[u8], size: Size, area: Block, rgba: &mut Vec<u8>) -> Result<()> {
    let row_bytes = area.width as usize * BYTES_PER_PIXEL;
    rgba.resize(row_bytes * area.height as usize, 0);
    let source = Pixels { bytes: bgra, stride: size.width as usize * BYTES_PER_PIXEL };
    copy_block(&source, area, &mut PixelsMut { bytes: rgba, stride: row_bytes }, (0, 0))?;
    for pixel in rgba.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
        let [red, green, blue] = rgb_of(*pixel);
        *pixel = [red, green, blue, OPAQUE];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::recorder::output_file::OutputFile;

    fn decoded_delays(path: &Path) -> Vec<u16> {
        let mut decoder = gif::DecodeOptions::new().read_info(File::open(path).unwrap()).unwrap();
        let mut delays = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            delays.push(frame.delay);
        }
        delays
    }

    #[test]
    fn long_idle_delays_are_split_without_losing_time() {
        assert_eq!(frame_delays(0).collect::<Vec<_>>(), [1]);
        assert_eq!(frame_delays(500).collect::<Vec<_>>(), [500]);
        assert_eq!(frame_delays(65_535).collect::<Vec<_>>(), [65_535]);
        assert_eq!(frame_delays(180_000).collect::<Vec<_>>(), [65_535, 65_535, 48_930]);
        assert_eq!(frame_delays(u64::MAX).map(u64::from).sum::<u64>(), MAX_TOTAL_DELAY);
    }

    #[test]
    fn a_gif_idle_for_twenty_minutes_keeps_its_exact_length() {
        let folder = std::env::temp_dir().join(format!("winpilot-gif-idle-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("idle.gif");
        let output = OutputFile::create_new(&path).unwrap();
        let size = Size::new(4, 2);
        let mut writer = GifWriter::create(output.path(), size, 1, Quality::Low).unwrap();
        writer.write(&[40; 32], 0).unwrap();
        writer.write(&[200; 32], 1200).unwrap();
        writer.finish(1201).unwrap();
        let delays = decoded_delays(&path);
        drop(output);
        let _ = fs::remove_dir_all(&folder);
        assert_eq!(delays, [65_535, 54_465, 100]);
        assert_eq!(delays.iter().map(|&delay| u64::from(delay)).sum::<u64>(), 120_100);
    }

    #[test]
    fn rgba_copies_swap_channels_inside_the_area_only() {
        let size = Size::new(3, 2);
        let bgra: Vec<u8> = (0..24).collect();
        let mut rgba = Vec::new();
        copy_as_rgba(&bgra, size, Block { x: 1, y: 1, width: 2, height: 1 }, &mut rgba).unwrap();
        assert_eq!(rgba, [18, 17, 16, 255, 22, 21, 20, 255]);
    }
}
