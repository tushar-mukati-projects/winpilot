use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, IMFSourceReader, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MFAudioFormat_AAC, MFCreateMediaType,
    MFCreateSourceReaderFromURL, MFMediaType_Audio, MFMediaType_Video, MFVideoFormat_NV12,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
use windows_core::{HSTRING, w};
use winpilot_core::{
    Rect, WindowRef,
    protocol::{Quality, VideoFormat},
};

use super::layout::Size;
use super::media_foundation::MediaFoundation;
use super::mp4::InputPixels;
use super::video_writer::{VideoWriter, WriterSettings};
use super::{EndReason, OutputFile, Recorder, RecordingSource, RecordingSpec, Redact, VideoSettings};
use crate::audio::AudioSource;
use crate::com::ComApartment;
use crate::frame::Frame;

const OUTPUT_FOLDER_VARIABLE: &str = "WINPILOT_RECORD_TEST_DIR";
const START_PATIENCE: Duration = Duration::from_secs(10);

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    folder: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::var_os(OUTPUT_FOLDER_VARIABLE).map_or_else(std::env::temp_dir, PathBuf::from);
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let folder = root.join(format!("winpilot-record-test-{name}-{}-{serial}", std::process::id()));
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
    VideoSettings { format, fps, quality: Quality::High, max_seconds: 60, cursor: true, max_width, audio: None }
}

fn spec(source: RecordingSource, path: &Path, video: VideoSettings) -> RecordingSpec {
    RecordingSpec {
        source,
        output: OutputFile::create_new(path).unwrap(),
        video,
        redactor: None,
        kill_switch: Box::new(|| false),
        stop: Arc::new(AtomicBool::new(false)),
    }
}

struct BlackOut {
    calls: Arc<AtomicUsize>,
}

impl Redact for BlackOut {
    fn refresh(&mut self) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed) == 0
    }

    fn cover(&self, frame: &mut Frame) {
        frame.bgra.fill(0);
    }
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

fn mp4_decoded_seconds(path: &Path) -> f64 {
    let _apartment = ComApartment::enter_any().unwrap();
    let _media_foundation = MediaFoundation::start().unwrap();
    let url = HSTRING::from(path.as_os_str());
    let reader = unsafe { MFCreateSourceReaderFromURL(&url, None::<&IMFAttributes>) }.unwrap();
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let mut end = 0i64;
    for _ in 0..100_000 {
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        unsafe { reader.ReadSample(stream, 0, None, Some(&raw mut flags), None, Some(&raw mut sample)) }.unwrap();
        if let Some(sample) = sample {
            let time = unsafe { sample.GetSampleTime() }.unwrap();
            let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(0);
            end = end.max(time + duration);
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
    }
    end as f64 / 10_000_000.0
}

fn mp4_audio_stream(path: &Path) -> (u32, u32, f64) {
    let _apartment = ComApartment::enter_any().unwrap();
    let _media_foundation = MediaFoundation::start().unwrap();
    let url = HSTRING::from(path.as_os_str());
    let reader = unsafe { MFCreateSourceReaderFromURL(&url, None::<&IMFAttributes>) }.unwrap();
    let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    let native = unsafe { reader.GetNativeMediaType(stream, 0) }.unwrap();
    assert_eq!(unsafe { native.GetGUID(&MF_MT_MAJOR_TYPE) }.unwrap(), MFMediaType_Audio);
    assert_eq!(unsafe { native.GetGUID(&MF_MT_SUBTYPE) }.unwrap(), MFAudioFormat_AAC);
    let rate = unsafe { native.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.unwrap();
    let channels = unsafe { native.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.unwrap();
    let mut end = 0i64;
    for _ in 0..100_000 {
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        unsafe { reader.ReadSample(stream, 0, None, Some(&raw mut flags), None, Some(&raw mut sample)) }.unwrap();
        if let Some(sample) = sample {
            let time = unsafe { sample.GetSampleTime() }.unwrap();
            end = end.max(time + unsafe { sample.GetSampleDuration() }.unwrap_or(0));
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
    }
    (rate, channels, end as f64 / 10_000_000.0)
}

fn first_luma(sample: &IMFSample) -> u8 {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }.unwrap();
    let mut data = std::ptr::null_mut();
    unsafe { buffer.Lock(&raw mut data, None, None) }.unwrap();
    let luma = unsafe { *data };
    unsafe { buffer.Unlock() }.unwrap();
    luma
}

fn nv12_reader(path: &Path) -> IMFSourceReader {
    let url = HSTRING::from(path.as_os_str());
    let reader = unsafe { MFCreateSourceReaderFromURL(&url, None::<&IMFAttributes>) }.unwrap();
    let wanted = unsafe { MFCreateMediaType() }.unwrap();
    unsafe { wanted.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }.unwrap();
    unsafe { wanted.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12) }.unwrap();
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    unsafe { reader.SetCurrentMediaType(stream, None, &wanted) }.unwrap();
    reader
}

fn decoded_lumas(path: &Path) -> Vec<(i64, u8)> {
    let _apartment = ComApartment::enter_any().unwrap();
    let _media_foundation = MediaFoundation::start().unwrap();
    let reader = nv12_reader(path);
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let mut lumas = Vec::new();
    for _ in 0..10_000 {
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        unsafe { reader.ReadSample(stream, 0, None, Some(&raw mut flags), None, Some(&raw mut sample)) }.unwrap();
        if let Some(sample) = sample {
            lumas.push((unsafe { sample.GetSampleTime() }.unwrap(), first_luma(&sample)));
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
    }
    lumas
}

fn ffprobe_seconds(path: &Path) -> Option<f64> {
    let output = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
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
    let begun = recorder.started().clone();
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
#[ignore = "records the screen and audio"]
fn records_system_audio_alongside_the_video_in_an_mp4() {
    let scratch = Scratch::new("audio-mp4");
    let path = scratch.file("with-sound.mp4");
    let primary = primary_monitor();
    let mut settings = video(VideoFormat::Mp4, 30, 1280);
    settings.audio = Some(AudioSource::System);
    let region = RecordingSource::Screen(Rect::new(primary.x, primary.y, 640, 360));
    let recorder = Recorder::start(spec(region, &path, settings), START_PATIENCE).unwrap();
    let audio = recorder.started().audio.clone();
    thread::sleep(Duration::from_secs(3));
    let finished = recorder.stop();
    eprintln!("recording: audio {audio:?}, finished {finished:?}");
    assert_eq!(finished.reason, EndReason::Stopped);
    assert!(finished.finalize_error.is_none() && finished.write_error.is_none(), "{finished:?}");
    assert_eq!(audio.source, Some(AudioSource::System), "{audio:?}");
    assert_eq!(finished.audio.source, Some(AudioSource::System));
    let (rate, channels, audio_seconds) = mp4_audio_stream(&path);
    let video_seconds = mp4_decoded_seconds(&path);
    eprintln!("recording: {rate} Hz x {channels}, audio {audio_seconds:.3} s, video {video_seconds:.3} s");
    assert_eq!((rate, channels), (48_000, 2));
    assert!(audio_seconds > 2.0, "{audio_seconds:.3} s of audio");
    assert!((audio_seconds - video_seconds).abs() < 0.1, "audio {audio_seconds:.3} s, video {video_seconds:.3} s");
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
    let redactor = Box::new(BlackOut { calls: Arc::clone(&calls) });
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
fn two_recordings_in_a_row_from_a_fresh_thread_both_finish() {
    let scratch = Scratch::new("two-in-a-row");
    let paths = [scratch.file("first.mp4"), scratch.file("second.gif")];
    let cycles = thread::spawn(move || {
        let primary = primary_monitor();
        let region = RecordingSource::Screen(Rect::new(primary.x, primary.y, 256, 144));
        let formats = [VideoFormat::Mp4, VideoFormat::Gif];
        paths
            .iter()
            .zip(formats)
            .map(|(path, format)| {
                let recorder = Recorder::start(spec(region, path, video(format, 15, 320)), START_PATIENCE).unwrap();
                thread::sleep(Duration::from_millis(400));
                recorder.stop()
            })
            .collect::<Vec<_>>()
    });
    for finished in cycles.join().unwrap() {
        assert_eq!(finished.reason, EndReason::Stopped);
        assert!(finished.bytes > 0 && finished.finalize_error.is_none(), "{finished:?}");
    }
}

#[test]
fn a_failed_start_removes_the_reserved_file_within_its_patience() {
    let scratch = Scratch::new("failed-start");
    let path = scratch.file("missing-window.mp4");
    let source = RecordingSource::Window { window: WindowRef(0x7FFF_FFF0), crop: None };
    let started = Instant::now();
    let error =
        Recorder::start(spec(source, &path, video(VideoFormat::Mp4, 30, 1920)), Duration::from_secs(2)).err().unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(error.code, winpilot_core::ErrorCode::NotFound);
    assert!(!path.exists());
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
    let begun = recorder.started().clone();
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
    let output = OutputFile::create_new(&settings.path).unwrap();
    output.keep();
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
fn an_mp4_dropped_without_finalizing_still_plays_up_to_its_last_fragment() {
    let _apartment = ComApartment::enter_any().unwrap();
    let scratch = Scratch::new("unfinalized-mp4");
    let size = Size::new(1280, 720);
    let settings = WriterSettings::new(scratch.file("killed.mp4"), VideoFormat::Mp4, size, 30, Quality::High);
    let output = OutputFile::create_new(&settings.path).unwrap();
    output.keep();
    let pictures: Vec<Vec<u8>> = (0..8).map(|step| synthetic_frame(size, step)).collect();
    let mut writer = VideoWriter::create(&settings).unwrap();
    for tick in 0..150 {
        writer.write(&pictures[tick % pictures.len()], tick as u64).unwrap();
    }
    drop(writer);
    let decoded = mp4_decoded_seconds(&settings.path);
    let (frame_size, has_sample) = mp4_frame_size(&settings.path);
    let probed = ffprobe_seconds(&settings.path);
    eprintln!("recording: unfinalized 5 s mp4 decodes {decoded:.2} s with Media Foundation, ffprobe says {probed:?}");
    assert_eq!(frame_size, size);
    assert!(has_sample);
    assert!(decoded >= 3.0, "only {decoded:.2} s of 5 s survived");
    if let Some(probed) = probed {
        assert!(probed >= 3.0, "ffprobe reports {probed:.2} s");
    }
}

#[test]
#[ignore = "encodes video files"]
fn skipped_ticks_hold_the_previous_frame_instead_of_showing_the_next_one_early() {
    let _apartment = ComApartment::enter_any().unwrap();
    let scratch = Scratch::new("gapped-mp4");
    let size = Size::new(320, 240);
    let gray = |tick: u64| [(tick * 8) as u8, (tick * 8) as u8, (tick * 8) as u8, 255].repeat(size.pixels());
    for input in [InputPixels::Rgb32, InputPixels::Nv12] {
        let mut settings = WriterSettings::new(
            scratch.file(&format!("gapped-{input:?}.mp4")),
            VideoFormat::Mp4,
            size,
            10,
            Quality::Max,
        );
        settings.mp4_inputs = vec![input];
        let output = OutputFile::create_new(&settings.path).unwrap();
        output.keep();
        let mut writer = VideoWriter::create(&settings).unwrap();
        for tick in (0..10).chain(20..30) {
            writer.write(&gray(tick), tick).unwrap();
        }
        writer.finish(30).unwrap();
        let lumas = decoded_lumas(&settings.path);
        let before_gap = lumas.iter().find(|(time, _)| *time == 9_000_000).unwrap().1;
        let after_gap = lumas.iter().find(|(time, _)| *time == 20_000_000).unwrap().1;
        let in_gap: Vec<u8> =
            lumas.iter().filter(|(time, _)| (10_000_000..20_000_000).contains(time)).map(|(_, luma)| *luma).collect();
        assert!(!in_gap.is_empty(), "{input:?}: {lumas:?}");
        assert!(after_gap > before_gap + 40, "{input:?}: {lumas:?}");
        assert!(in_gap.iter().all(|luma| luma.abs_diff(before_gap) <= 4), "{input:?} shows content early: {lumas:?}");
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
