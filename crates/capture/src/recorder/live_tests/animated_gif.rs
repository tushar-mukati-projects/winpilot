use super::*;

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
