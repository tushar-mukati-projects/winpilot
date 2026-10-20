use super::*;

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
