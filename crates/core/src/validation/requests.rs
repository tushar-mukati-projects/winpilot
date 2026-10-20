use crate::{
    error::{Error, Result},
    field_rules::only_one,
    observe::OcrMode,
    observe_request::{ScreenshotRequest, TextRequest},
    record_plan::{RECORD_SECONDS, RECORD_WIDTHS},
    recording::{RecordAction, RecordAudio, RecordRequest, VideoFormat},
};

use super::{JPEG_QUALITIES, SCREENSHOT_WIDTHS, check_optional, check_screen_rect};

const EMPTY_RECORD_PATH: &str = r"path is empty; pass a full path like C:\clips\demo.mp4 or leave it out";

impl ScreenshotRequest {
    pub fn validate(&self) -> Result<()> {
        check_screen_rect("region", self.region)?;
        check_optional("max_width", self.max_width, &SCREENSHOT_WIDTHS)?;
        check_optional("quality", self.quality, &JPEG_QUALITIES)
    }
}

impl TextRequest {
    pub fn validate(&self) -> Result<()> {
        check_screen_rect("region", self.region)?;
        if self.region.is_some() && self.ocr == OcrMode::Never {
            return Err(Error::invalid("a region can only be read with OCR; drop ocr=never"));
        }
        if self.coords && self.ocr == OcrMode::Never {
            return Err(Error::invalid("coords come from OCR; drop ocr=never"));
        }
        Ok(())
    }
}

impl RecordRequest {
    pub fn validate(&self) -> Result<()> {
        if self.action != RecordAction::Start {
            return check_no_start_options(self);
        }
        check_one_source(self)?;
        check_screen_rect("region", self.region)?;
        let fps_limit = self.format.unwrap_or(VideoFormat::Mp4).max_fps();
        check_optional("fps", self.fps, &(1..=fps_limit))?;
        check_optional("max_seconds", self.max_seconds, &RECORD_SECONDS)?;
        check_optional("max_width", self.max_width, &RECORD_WIDTHS)?;
        check_record_path(self.path.as_deref(), self.format)?;
        check_record_audio(self)
    }
}

fn check_record_audio(request: &RecordRequest) -> Result<()> {
    let Some(format) = VideoFormat::of_request(request.path.as_deref(), request.format) else { return Ok(()) };
    RecordAudio::for_format(request.audio, format).map(drop)
}

fn check_no_start_options(request: &RecordRequest) -> Result<()> {
    let given = request.start_options();
    if given.is_empty() {
        return Ok(());
    }
    let action = if request.action == RecordAction::Stop { "stop" } else { "status" };
    Err(Error::invalid(format!("mode=record action={action} takes no other fields; remove {}", given.join(", "))))
}

fn check_one_source(request: &RecordRequest) -> Result<()> {
    only_one(&[
        ("target", request.target.is_some()),
        ("region", request.region.is_some()),
        ("monitor", request.monitor.is_some()),
    ])
}

fn check_record_path(path: Option<&str>, format: Option<VideoFormat>) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    if path.trim().is_empty() {
        return Err(Error::invalid(EMPTY_RECORD_PATH));
    }
    VideoFormat::for_path(path, format).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geometry::Rect, validation::tests::invalid_message};

    #[test]
    fn screenshot_width_quality_and_region_are_range_checked() {
        let shot = |max_width, quality| ScreenshotRequest { max_width, quality, ..ScreenshotRequest::default() };
        assert!(shot(Some(64), Some(1)).validate().is_ok());
        assert!(shot(Some(2576), Some(100)).validate().is_ok());
        assert_eq!(invalid_message(shot(Some(63), None).validate()), "max_width must be from 64 to 2576, got 63");
        assert_eq!(invalid_message(shot(None, Some(101)).validate()), "quality must be from 1 to 100, got 101");
        let far =
            ScreenshotRequest { region: Some(Rect::new(i32::MAX - 5, 0, 100, 100)), ..ScreenshotRequest::default() };
        assert_eq!(
            invalid_message(far.validate()),
            format!("region {},0 100x100 needs a positive size and must stay within ±100000 pixels", i32::MAX - 5)
        );
        let empty = TextRequest { region: Some(Rect::new(0, 0, 0, 10)), ..TextRequest::default() };
        assert!(empty.validate().is_err());
        let blind =
            TextRequest { region: Some(Rect::new(0, 0, 10, 10)), ocr: OcrMode::Never, ..TextRequest::default() };
        assert_eq!(invalid_message(blind.validate()), "a region can only be read with OCR; drop ocr=never");
    }

    fn start() -> RecordRequest {
        RecordRequest::new(RecordAction::Start)
    }

    #[test]
    fn record_start_options_are_range_checked() {
        let cases = [
            (RecordRequest { fps: Some(0), ..start() }, "fps must be from 1 to 60, got 0"),
            (
                RecordRequest { fps: Some(21), format: Some(VideoFormat::Gif), ..start() },
                "fps must be from 1 to 20, got 21",
            ),
            (RecordRequest { max_seconds: Some(1801), ..start() }, "max_seconds must be from 1 to 1800, got 1801"),
            (RecordRequest { max_width: Some(159), ..start() }, "max_width must be from 160 to 3840, got 159"),
            (
                RecordRequest { target: Some("w1".to_string()), monitor: Some(0), ..start() },
                "give only one of target, region or monitor; got target and monitor",
            ),
            (RecordRequest { path: Some("C:\\v\\a.webm".to_string()), ..start() }, "path must end in .mp4 or .gif"),
            (
                RecordRequest { path: Some("  ".to_string()), ..start() },
                "path is empty; pass a full path like C:\\clips\\demo.mp4 or leave it out",
            ),
            (
                RecordRequest { path: Some("C:\\v\\a.gif".to_string()), format: Some(VideoFormat::Mp4), ..start() },
                "path ends in .gif but format is mp4; make them match",
            ),
        ];
        for (request, expected) in cases {
            assert_eq!(invalid_message(request.validate()), expected);
        }
        assert!(start().validate().is_ok());
        let gif = RecordRequest { path: Some("C:\\v\\a.GIF".to_string()), fps: Some(20), ..start() };
        assert!(gif.validate().is_ok());
    }

    #[test]
    fn record_audio_is_refused_for_gif_and_accepted_for_mp4() {
        let gif = RecordRequest { format: Some(VideoFormat::Gif), audio: Some(RecordAudio::System), ..start() };
        assert_eq!(invalid_message(gif.validate()), "gif recordings have no sound; drop audio=system or record mp4");
        let gif_path =
            RecordRequest { path: Some("C:\\v\\a.gif".to_string()), audio: Some(RecordAudio::Both), ..start() };
        assert_eq!(invalid_message(gif_path.validate()), "gif recordings have no sound; drop audio=both or record mp4");
        let silent_gif = RecordRequest { format: Some(VideoFormat::Gif), audio: Some(RecordAudio::None), ..start() };
        assert!(silent_gif.validate().is_ok());
        for audio in [RecordAudio::System, RecordAudio::Mic, RecordAudio::Both, RecordAudio::None] {
            assert!(RecordRequest { audio: Some(audio), ..start() }.validate().is_ok());
            let mp4 = RecordRequest { format: Some(VideoFormat::Mp4), audio: Some(audio), ..start() };
            assert!(mp4.validate().is_ok());
        }
    }

    #[test]
    fn record_stop_and_status_take_no_start_options() {
        assert!(RecordRequest::new(RecordAction::Stop).validate().is_ok());
        let noisy = RecordRequest {
            fps: Some(10),
            path: Some("C:\\a.mp4".to_string()),
            ..RecordRequest::new(RecordAction::Stop)
        };
        assert_eq!(
            invalid_message(noisy.validate()),
            "mode=record action=stop takes no other fields; remove fps, path"
        );
        let stop_audio = RecordRequest { audio: Some(RecordAudio::Mic), ..RecordRequest::new(RecordAction::Stop) };
        assert_eq!(
            invalid_message(stop_audio.validate()),
            "mode=record action=stop takes no other fields; remove audio"
        );
        let status = RecordRequest { monitor: Some(0), ..RecordRequest::new(RecordAction::Status) };
        assert_eq!(
            invalid_message(status.validate()),
            "mode=record action=status takes no other fields; remove monitor"
        );
    }
}
