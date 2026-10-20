use super::*;

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
