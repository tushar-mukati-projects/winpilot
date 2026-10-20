use super::*;

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
#[ignore = "records the screen"]
fn a_window_recording_matches_the_visible_frame_bounds() {
    crate::dpi::make_thread_per_monitor_aware().unwrap();
    let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else { return };
    let bounds = frame_bounds(taskbar).unwrap();
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
    let (width, height) = (bounds.width as u32, bounds.height as u32);
    eprintln!("recording: taskbar bounds {width}x{height}, recorded {}x{}", begun.width, begun.height);
    assert_eq!((begun.width, begun.height), (width & !1, height & !1));
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
