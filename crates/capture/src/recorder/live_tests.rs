mod animated_gif;
mod audio;
mod mp4;
mod redaction;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, IMFSourceReader, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MFAudioFormat_AAC, MFCreateMediaType,
    MFCreateSourceReaderFromURL, MFMediaType_Audio, MFMediaType_Video, MFVideoFormat_NV12,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
use windows::core::{HSTRING, w};
use winpilot_core::{
    Rect, WindowRef,
    protocol::{Quality, VideoFormat},
};
use winpilot_win32::{ComApartment, frame_bounds};

use super::layout::Size;
use super::mp4::InputPixels;
use super::video_writer::{VideoWriter, WriterSettings};
use super::{EndReason, OutputFile, Recorder, RecordingSource, RecordingSpec, Redact, VideoSettings};
use crate::audio::AudioSource;
use crate::frame::Frame;
use winpilot_win32::MediaFoundation;

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

fn primary_monitor() -> Rect {
    crate::dpi::make_thread_per_monitor_aware().unwrap();
    super::monitor_area::MonitorArea::locate(Rect::new(0, 0, 1, 1)).unwrap().monitor_rect
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
