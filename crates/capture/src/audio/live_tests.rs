use std::time::{Duration, Instant};

use deckhand_core::{Error, ErrorCode};

use super::{AudioCapture, AudioSource, capture_audio};

#[test]
#[ignore = "records audio"]
fn system_audio_is_captured_for_exactly_the_asked_duration_even_when_silent() {
    let pcm = capture_audio(AudioSource::System, Duration::from_secs(2), || Ok(())).unwrap();
    eprintln!("audio: {} Hz, {} channels, {:.2} s", pcm.sample_rate, pcm.channels, pcm.seconds());
    assert!(pcm.sample_rate >= 8_000 && pcm.channels >= 1);
    assert_eq!(pcm.frames(), pcm.sample_rate as usize * 2);
    assert!(pcm.samples.iter().all(|sample| sample.is_finite() && sample.abs() <= 1.0));
}

#[test]
#[ignore = "records audio"]
fn the_check_callback_aborts_a_capture() {
    let started = Instant::now();
    let error = capture_audio(AudioSource::System, Duration::from_secs(30), || {
        Err(Error::new(ErrorCode::StoppedByUser, "stopped"))
    })
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::StoppedByUser);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
#[ignore = "records audio"]
fn a_streaming_capture_delivers_contiguous_chunks_and_stops_cleanly() {
    let capture = AudioCapture::start(AudioSource::System).unwrap();
    std::thread::sleep(Duration::from_millis(800));
    let format = capture.format();
    let mut chunks: Vec<_> = capture.chunks().try_iter().collect();
    let end = capture.stop();
    assert!(end.error.is_none(), "{:?}", end.error);
    chunks.extend(end.chunks);
    let mut expected = 0;
    for chunk in &chunks {
        assert_eq!(chunk.first_frame, expected);
        expected += (chunk.samples.len() / format.channel_count()) as u64;
    }
    let seconds = expected as f64 / f64::from(format.sample_rate);
    assert!((0.7..1.5).contains(&seconds), "{seconds:.2} s");
}
