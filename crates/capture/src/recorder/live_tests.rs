use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use deckhand_core::{
    Rect, WindowRef,
    protocol::{Quality, VideoFormat},
};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, MF_MT_FRAME_SIZE, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MFCreateSourceReaderFromURL,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
use windows_core::{HSTRING, w};

use super::layout::Size;
use super::media_foundation::MediaFoundation;
use super::mp4::InputPixels;
use super::video_writer::{VideoWriter, WriterSettings};
use super::{EndReason, Recorder, RecordingSource, RecordingSpec, VideoSettings};
use crate::com::ComApartment;
use crate::frame::Frame;

const OUTPUT_FOLDER_VARIABLE: &str = "DECKHAND_RECORD_TEST_DIR";
const START_PATIENCE: Duration = Duration::from_secs(10);

struct Scratch {
    folder: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::var_os(OUTPUT_FOLDER_VARIABLE)
            .map_or_else(|| std::env::temp_dir().join("deckhand-record-tests"), PathBuf::from);
        let folder = root.join(name);
        fs::create_dir_all(&folder).unwrap();
        Self { folder }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.folder.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.folder);
    }
}

fn video(format: VideoFormat, fps: u32, max_width: u32) -> VideoSettings {
    VideoSettings { format, fps, quality: Quality::High, max_seconds: 60, cursor: true, max_width }
}

fn spec(source: RecordingSource, path: &Path, video: VideoSettings) -> RecordingSpec {
    RecordingSpec { source, path: path.to_path_buf(), video, redactor: None, kill_switch: Box::new(|| false) }
}

fn primary_monitor() -> Rect {
    crate::dpi::make_thread_per_monitor_aware().unwrap();
    super::monitor_area::MonitorArea::locate(Rect::new(0, 0, 1, 1)).unwrap().monitor_rect
}

fn mp4_frame_size(path: &Path) -> (Size, bool) {
    let _apartment = ComApartment::enter_any().unwrap();
    let _media_foundation = MediaFoundation::start().unwrap();
    let url = HSTRING::from(path.as_os_str());
    let reader = unsafe { MFCreateSourceReaderFromURL(&url, None::<&IMFAttributes>) }.unwrap();
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let native = unsafe { reader.GetNativeMediaType(stream, 0) }.unwrap();
    let packed = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap();
    let mut flags = 0u32;
    let mut sample: Option<IMFSample> = None;
    unsafe { reader.ReadSample(stream, 0, None, Some(&raw mut flags), None, Some(&raw mut sample)) }.unwrap();
    (Size::new((packed >> 32) as u32, packed as u32), sample.is_some())
}

fn gif_frames(path: &Path) -> (Size, Vec<gif::Frame<'static>>) {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(fs::File::open(path).unwrap()).unwrap();
    let size = Size::new(u32::from(decoder.width()), u32::from(decoder.height()));
    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().unwrap() {
        frames.push(frame.clone());
    }
    (size, frames)
}

fn report(label: &str, finished: &super::Finished, wall: Duration) {
    eprintln!(
        "recording: {label}: {} frames, {} dropped, {:.2} s of video in {:.2} s, {} bytes, {:?}",
        finished.frames,
        finished.dropped,
        finished.seconds,
        wall.as_secs_f64(),
        finished.bytes,
        finished.reason
    );
}

#[test]
#[ignore = "records the screen"]
fn records_the_primary_monitor_to_a_valid_mp4() {
    let scratch = Scratch::new("monitor-mp4");
    let path = scratch.file("monitor.mp4");
    let primary = primary_monitor();
    let started = Instant::now();
    let recorder = Recorder::start(
        spec(RecordingSource::Screen(primary), &path, video(VideoFormat::Mp4, 30, 1920)),
        START_PATIENCE,
    )
    .unwrap();
    let begun = recorder.started();
    thread::sleep(Duration::from_secs(4));
    let progress = recorder.progress();
    assert!(progress.frames > 0 && !recorder.has_ended());
    let finished = recorder.stop();
    report("primary monitor mp4 @ 30 fps", &finished, started.elapsed());
    assert_eq!(finished.reason, EndReason::Stopped);
    assert!(finished.bytes > 0 && finished.finalize_error.is_none());
    assert!(finished.dropped * 10 <= finished.frames, "{finished:?}");
    let (size, has_sample) = mp4_frame_size(&path);
    assert_eq!(size, Size::new(begun.width, begun.height));
    assert!(has_sample);
    assert!(begun.width <= 1920 && begun.width.is_multiple_of(2) && begun.height.is_multiple_of(2));
}

#[test]
#[ignore = "records the screen"]
fn records_a_region_to_a_decodable_gif_and_stops_at_the_time_limit() {
    let scratch = Scratch::new("region-gif");
    let path = scratch.file("region.gif");
    let primary = primary_monitor();
    let region = Rect::new(primary.x + 100, primary.y + 100, 401, 301);
    let mut settings = video(VideoFormat::Gif, 10, 960);
    settings.max_seconds = 2;
    let recorder = Recorder::start(spec(RecordingSource::Screen(region), &path, settings), START_PATIENCE).unwrap();
    assert!(!recorder.started().clipped);
    let started = Instant::now();
    while !recorder.has_ended() && started.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(50));
    }
    let finished = recorder.stop();
    report("region gif @ 10 fps", &finished, started.elapsed());
    assert_eq!(finished.reason, EndReason::TimeLimit(2));
    assert_eq!(finished.frames + finished.dropped, 20);
    let (size, frames) = gif_frames(&path);
    assert_eq!(size, Size::new(400, 300));
    assert!(!frames.is_empty());
    let total_delay: u32 = frames.iter().map(|frame| u32::from(frame.delay)).sum();
    assert_eq!(total_delay, 200);
}

#[test]
#[ignore = "records the screen"]
fn screen_recordings_are_redacted_and_obey_the_kill_switch() {
    let scratch = Scratch::new("redacted-gif");
    let path = scratch.file("redacted.gif");
    let primary = primary_monitor();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let redactor = Box::new(move |frame: &mut Frame| {
        counted.fetch_add(1, Ordering::Relaxed);
        frame.bgra.fill(0);
    });
    let ticks = Arc::new(AtomicUsize::new(0));
    let ticked = Arc::clone(&ticks);
    let kill_switch = Box::new(move || ticked.fetch_add(1, Ordering::Relaxed) >= 15);
    let mut recording = spec(
        RecordingSource::Screen(Rect::new(primary.x, primary.y, 320, 200)),
        &path,
        video(VideoFormat::Gif, 20, 960),
    );
    recording.redactor = Some(redactor);
    recording.kill_switch = kill_switch;
    let recorder = Recorder::start(recording, START_PATIENCE).unwrap();
    let started = Instant::now();
    while !recorder.has_ended() && started.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(20));
    }
    let finished = recorder.stop();
    assert_eq!(finished.reason, EndReason::KillSwitch);
    assert!(calls.load(Ordering::Relaxed) >= 1);
    let (_, frames) = gif_frames(&path);
    assert!(frames.iter().all(|frame| frame.buffer.as_chunks::<4>().0.iter().all(|pixel| pixel[..3] == [0, 0, 0])));
}

#[test]
#[ignore = "records the screen"]
fn a_window_recording_matches_the_visible_frame_bounds() {
    crate::dpi::make_thread_per_monitor_aware().unwrap();
    let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else { return };
    let mut bounds = RECT::default();
    let size = size_of::<RECT>() as u32;
    unsafe { DwmGetWindowAttribute(taskbar, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut bounds).cast(), size) }.unwrap();
    let scratch = Scratch::new("window-mp4");
    let path = scratch.file("window.mp4");
    let source = RecordingSource::Window { window: WindowRef(taskbar.0 as i64), crop: None };
    let recorder = match Recorder::start(spec(source, &path, video(VideoFormat::Mp4, 15, 3840)), START_PATIENCE) {
        Ok(recorder) => recorder,
        Err(error) => {
            eprintln!("recording: the taskbar cannot be recorded as a window here: {error}");
            return;
        }
    };
    let begun = recorder.started();
    thread::sleep(Duration::from_millis(600));
    let finished = recorder.stop();
    assert_eq!(finished.reason, EndReason::Stopped);
    let (width, height) = ((bounds.right - bounds.left) as u32, (bounds.bottom - bounds.top) as u32);
    eprintln!("recording: taskbar bounds {width}x{height}, recorded {}x{}", begun.width, begun.height);
    assert_eq!((begun.width, begun.height), (width & !1, height & !1));
}

fn synthetic_frame(size: Size, step: usize) -> Vec<u8> {
    let mut bgra = vec![0u8; size.bgra_len()];
    for (index, pixel) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (index % size.width as usize, index / size.width as usize);
        *pixel = [((x + step * 8) % 256) as u8, ((y + step * 3) % 256) as u8, ((x ^ y) % 256) as u8, 255];
    }
    bgra
}

fn time_writer(settings: &WriterSettings, frames: usize) -> f64 {
    let pictures: Vec<Vec<u8>> = (0..8).map(|step| synthetic_frame(settings.size, step)).collect();
    let mut writer = VideoWriter::create(settings).unwrap();
    let started = Instant::now();
    for tick in 0..frames {
        writer.write(&pictures[tick % pictures.len()], tick as u64).unwrap();
    }
    writer.finish(frames as u64).unwrap();
    started.elapsed().as_secs_f64() * 1000.0 / frames as f64
}

#[test]
#[ignore = "encodes video files"]
fn mp4_encoding_keeps_up_at_1920x1200_and_30_fps() {
    let _apartment = ComApartment::enter_any().unwrap();
    let scratch = Scratch::new("mp4-timing");
    let size = Size::new(1920, 1200);
    for input in [InputPixels::Rgb32, InputPixels::Nv12] {
        let mut settings =
            WriterSettings::new(scratch.file(&format!("{input:?}.mp4")), VideoFormat::Mp4, size, 30, Quality::High);
        settings.mp4_inputs = vec![input];
        let per_frame = time_writer(&settings, 150);
        let bytes = fs::metadata(&settings.path).unwrap().len();
        eprintln!(
            "timing: mp4 1920x1200 @ 30 fps with {input:?} input: {per_frame:.2} ms per frame, {bytes} bytes for 5 s"
        );
        assert!(per_frame < 1000.0 / 30.0, "{input:?} input needs {per_frame:.2} ms per frame");
    }
}

#[test]
#[ignore = "encodes video files"]
fn gif_encoding_speed_per_quality() {
    let scratch = Scratch::new("gif-timing");
    let size = Size::new(960, 600);
    for quality in [Quality::Low, Quality::Medium, Quality::High, Quality::Max] {
        let path = scratch.file(&format!("{}.gif", quality.name()));
        let settings = WriterSettings::new(path, VideoFormat::Gif, size, 20, quality);
        let per_frame = time_writer(&settings, 20);
        let bytes = fs::metadata(&settings.path).unwrap().len();
        eprintln!(
            "timing: gif 960x600 full-frame changes, quality {}: {per_frame:.1} ms per frame, {bytes} bytes for 1 s",
            quality.name()
        );
    }
}
