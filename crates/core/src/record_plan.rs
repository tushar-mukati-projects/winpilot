use std::ops::RangeInclusive;

use crate::{
    config::RecordConfig,
    error::{Error, Result},
    quality::Quality,
    recording::{RecordArgs, VideoFormat},
};

pub const RECORD_SECONDS: RangeInclusive<u32> = 1..=1800;
pub const RECORD_WIDTHS: RangeInclusive<u32> = 160..=3840;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordPlan {
    pub format: VideoFormat,
    pub fps: u32,
    pub quality: Quality,
    pub max_seconds: u32,
    pub cursor: bool,
    pub max_width: u32,
}

pub fn record_plan(args: &RecordArgs, config: &RecordConfig) -> Result<RecordPlan> {
    let format = record_format(args, config)?;
    Ok(RecordPlan {
        format,
        fps: record_fps(args.fps, config.fps, format)?,
        quality: args.quality.unwrap_or(config.quality),
        max_seconds: clamped(args.max_seconds.unwrap_or(config.max_seconds), &RECORD_SECONDS),
        cursor: args.cursor.unwrap_or(config.cursor),
        max_width: clamped(args.max_width.or(config.max_width).unwrap_or(format.default_max_width()), &RECORD_WIDTHS),
    })
}

fn record_format(args: &RecordArgs, config: &RecordConfig) -> Result<VideoFormat> {
    match args.path.as_deref() {
        Some(path) => VideoFormat::for_path(path, args.format),
        None => Ok(args.format.unwrap_or(config.format)),
    }
}

fn record_fps(asked: Option<u32>, configured: u32, format: VideoFormat) -> Result<u32> {
    let allowed = 1..=format.max_fps();
    match asked {
        Some(fps) if !allowed.contains(&fps) => {
            Err(Error::invalid(format!("{} fps must be from 1 to {}, got {fps}", format.extension(), format.max_fps())))
        }
        Some(fps) => Ok(fps),
        None => Ok(clamped(configured, &allowed)),
    }
}

fn clamped(value: u32, allowed: &RangeInclusive<u32>) -> u32 {
    value.clamp(*allowed.start(), *allowed.end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::ErrorCode, recording::RecordAction};

    fn start() -> RecordArgs {
        RecordArgs::new(RecordAction::Start)
    }

    fn plan(args: &RecordArgs) -> RecordPlan {
        record_plan(args, &RecordConfig::default()).unwrap()
    }

    fn invalid(args: &RecordArgs, config: &RecordConfig) -> String {
        let error = record_plan(args, config).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        error.message
    }

    #[test]
    fn defaults_come_from_the_config() {
        let found = plan(&start());
        assert_eq!(
            found,
            RecordPlan {
                format: VideoFormat::Mp4,
                fps: 30,
                quality: Quality::High,
                max_seconds: 300,
                cursor: true,
                max_width: 1920
            }
        );
    }

    #[test]
    fn gif_defaults_cap_fps_at_twenty_and_width_at_960() {
        let gif = plan(&RecordArgs { format: Some(VideoFormat::Gif), ..start() });
        assert_eq!((gif.format, gif.fps, gif.max_width), (VideoFormat::Gif, 20, 960));
        let configured = RecordConfig { format: VideoFormat::Gif, fps: 12, ..RecordConfig::default() };
        let from_config = record_plan(&start(), &configured).unwrap();
        assert_eq!((from_config.format, from_config.fps, from_config.max_width), (VideoFormat::Gif, 12, 960));
    }

    #[test]
    fn every_argument_overrides_its_config_value() {
        let args = RecordArgs {
            format: Some(VideoFormat::Gif),
            fps: Some(10),
            quality: Some(Quality::Low),
            max_seconds: Some(15),
            cursor: Some(false),
            max_width: Some(640),
            ..start()
        };
        let config = RecordConfig { max_width: Some(1280), ..RecordConfig::default() };
        let found = record_plan(&args, &config).unwrap();
        assert_eq!(
            found,
            RecordPlan {
                format: VideoFormat::Gif,
                fps: 10,
                quality: Quality::Low,
                max_seconds: 15,
                cursor: false,
                max_width: 640
            }
        );
    }

    #[test]
    fn configured_max_width_applies_to_both_formats() {
        let config = RecordConfig { max_width: Some(1280), ..RecordConfig::default() };
        assert_eq!(record_plan(&start(), &config).unwrap().max_width, 1280);
        let gif = RecordArgs { format: Some(VideoFormat::Gif), ..start() };
        assert_eq!(record_plan(&gif, &config).unwrap().max_width, 1280);
    }

    #[test]
    fn out_of_range_config_values_are_clamped() {
        let config = RecordConfig { fps: 500, max_seconds: 0, max_width: Some(99_999), ..RecordConfig::default() };
        let found = record_plan(&start(), &config).unwrap();
        assert_eq!((found.fps, found.max_seconds, found.max_width), (60, 1, 3840));
        let zero = RecordConfig { fps: 0, max_width: Some(1), ..RecordConfig::default() };
        let found = record_plan(&start(), &zero).unwrap();
        assert_eq!((found.fps, found.max_width), (1, 160));
    }

    #[test]
    fn the_path_extension_picks_the_format() {
        let gif = plan(&RecordArgs { path: Some("C:\\v\\clip.GIF".to_string()), ..start() });
        assert_eq!((gif.format, gif.fps), (VideoFormat::Gif, 20));
        let config = RecordConfig { format: VideoFormat::Gif, ..RecordConfig::default() };
        let mp4 = RecordArgs { path: Some("C:\\v\\clip.mp4".to_string()), ..start() };
        assert_eq!(record_plan(&mp4, &config).unwrap().format, VideoFormat::Mp4);
    }

    #[test]
    fn a_mismatched_or_unknown_extension_is_invalid() {
        let config = RecordConfig::default();
        let mismatch =
            RecordArgs { path: Some("C:\\v\\clip.mp4".to_string()), format: Some(VideoFormat::Gif), ..start() };
        assert_eq!(invalid(&mismatch, &config), "path ends in .mp4 but format is gif; make them match");
        let unknown = RecordArgs { path: Some("C:\\v\\clip.avi".to_string()), ..start() };
        assert_eq!(invalid(&unknown, &config), "path must end in .mp4 or .gif");
    }

    #[test]
    fn asked_fps_above_the_gif_limit_is_invalid_even_when_the_format_comes_from_config() {
        let config = RecordConfig { format: VideoFormat::Gif, ..RecordConfig::default() };
        assert_eq!(invalid(&RecordArgs { fps: Some(30), ..start() }, &config), "gif fps must be from 1 to 20, got 30");
        assert_eq!(record_plan(&RecordArgs { fps: Some(20), ..start() }, &config).unwrap().fps, 20);
        let mp4 = RecordArgs { fps: Some(61), ..start() };
        assert_eq!(invalid(&mp4, &RecordConfig::default()), "mp4 fps must be from 1 to 60, got 61");
    }
}
