use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use winpilot_core::{
    ErrorCode, Rect,
    protocol::{RecordAction, RecordReport, RecordRequest, RecordState, VideoFormat},
};

use super::run;
use crate::{call_scope::CallScope, test_fixture::engine};

const OUTPUT_FOLDER_VARIABLE: &str = "WINPILOT_RECORD_TEST_DIR";

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::var_os(OUTPUT_FOLDER_VARIABLE).map_or_else(std::env::temp_dir, PathBuf::from);
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let folder = root.join(format!("winpilot-record-test-{name}-{}-{serial}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        Self(folder)
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn record(args: &RecordRequest) -> winpilot_core::Result<RecordReport> {
    run(engine(), args, &CallScope::relaxed())
}

fn action(action: RecordAction) -> RecordReport {
    record(&RecordRequest::new(action)).unwrap()
}

fn starts_with(path: &str, magic: &[u8], offset: usize) -> bool {
    fs::read(Path::new(path)).is_ok_and(|bytes| bytes.get(offset..offset + magic.len()) == Some(magic))
}

fn region_gif(path: String) -> RecordRequest {
    let region = Rect::new(50, 50, 320, 200);
    RecordRequest {
        region: Some(region),
        format: Some(VideoFormat::Gif),
        fps: Some(10),
        path: Some(path),
        ..RecordRequest::new(RecordAction::Start)
    }
}

#[test]
#[ignore = "records the screen"]
fn one_recording_at_a_time_from_start_to_saved_file() {
    let scratch = Scratch::new("worker-lifecycle");
    let _ = action(RecordAction::Stop);
    let path = scratch.file("lifecycle.gif");
    let started = record(&region_gif(path.clone())).unwrap();
    assert_eq!(started.state, RecordState::Recording);
    let reported = started.path.clone().unwrap_or_default();
    assert_eq!(Path::new(&reported), Path::new(&path), "{reported}");
    assert_eq!((started.width, started.height, started.fps), (320, 200, 10));
    let busy = record(&region_gif(scratch.file("second.gif"))).unwrap_err();
    assert_eq!(busy.code, ErrorCode::Busy);
    assert!(busy.message.contains(&reported), "{}", busy.message);
    assert!(!Path::new(&scratch.file("second.gif")).exists());
    thread::sleep(Duration::from_millis(1200));
    let status = action(RecordAction::Status);
    assert_eq!(status.state, RecordState::Recording);
    assert!(status.frames > 0 && status.seconds > 0.0);
    let saved = action(RecordAction::Stop);
    assert_eq!(saved.state, RecordState::Finished);
    assert!(saved.bytes > 0 && saved.note.is_none(), "{saved:?}");
    assert!(starts_with(&path, b"GIF89a", 0));
    assert_eq!(action(RecordAction::Stop), RecordReport::idle());
    let taken = record(&region_gif(path)).unwrap_err();
    assert_eq!(taken.code, ErrorCode::InvalidArgument);
}

#[test]
#[ignore = "records the screen"]
fn a_monitor_mp4_stops_itself_at_max_seconds() {
    let scratch = Scratch::new("worker-monitor");
    let _ = action(RecordAction::Stop);
    let path = scratch.file("monitor.mp4");
    let args = RecordRequest {
        monitor: Some(0),
        max_seconds: Some(1),
        path: Some(path.clone()),
        ..RecordRequest::new(RecordAction::Start)
    };
    let started = record(&args).unwrap();
    assert_eq!(started.fps, 30);
    assert!(started.width <= 1920 && started.width.is_multiple_of(2) && started.height.is_multiple_of(2));
    let waiting = Instant::now();
    while action(RecordAction::Status).state == RecordState::Recording && waiting.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(100));
    }
    let finished = action(RecordAction::Status);
    assert_eq!(finished.state, RecordState::Finished);
    let note = finished.note.as_deref().unwrap_or_default();
    assert!(note.starts_with("stopped automatically at max_seconds (1 s)"), "{note}");
    assert_eq!(action(RecordAction::Status), finished);
    assert_eq!(action(RecordAction::Stop), finished);
    assert!(finished.bytes > 0 && starts_with(&path, b"ftyp", 4));
}
