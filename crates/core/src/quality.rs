use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{config::ScreenshotConfig, protocol::ScreenshotArgs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Low,
    Medium,
    High,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageQuality {
    pub max_width: Option<u32>,
    pub quality: u8,
}

impl Quality {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    pub const fn screenshot_width(self) -> Option<u32> {
        match self {
            Self::Low => Some(960),
            Self::Medium => Some(1280),
            Self::High => Some(1600),
            Self::Max => None,
        }
    }

    pub const fn screenshot_quality(self) -> u8 {
        match self {
            Self::Low => 60,
            Self::Medium => 80,
            Self::High => 88,
            Self::Max => 95,
        }
    }
}

pub fn screenshot_quality(args: &ScreenshotArgs, config: &ScreenshotConfig) -> ImageQuality {
    let max_width = args
        .max_width
        .map(Some)
        .or_else(|| args.preset.map(Quality::screenshot_width))
        .or_else(|| config.max_width.map(Some))
        .unwrap_or_else(|| config.preset.screenshot_width());
    let quality = args
        .quality
        .or_else(|| args.preset.map(Quality::screenshot_quality))
        .or(config.quality)
        .unwrap_or_else(|| config.preset.screenshot_quality());
    ImageQuality { max_width, quality }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(preset: Option<Quality>, max_width: Option<u32>, quality: Option<u8>) -> ScreenshotArgs {
        ScreenshotArgs { preset, max_width, quality, ..ScreenshotArgs::default() }
    }

    fn config(preset: Quality, max_width: Option<u32>, quality: Option<u8>) -> ScreenshotConfig {
        ScreenshotConfig { preset, max_width, quality }
    }

    fn resolved(args: &ScreenshotArgs, config: &ScreenshotConfig) -> (Option<u32>, u8) {
        let found = screenshot_quality(args, config);
        (found.max_width, found.quality)
    }

    #[test]
    fn built_in_default_is_the_medium_preset() {
        assert_eq!(resolved(&ScreenshotArgs::default(), &ScreenshotConfig::default()), (Some(1280), 80));
    }

    #[test]
    fn presets_map_to_width_and_quality() {
        let none = config(Quality::Medium, None, None);
        assert_eq!(resolved(&args(Some(Quality::Low), None, None), &none), (Some(960), 60));
        assert_eq!(resolved(&args(Some(Quality::Medium), None, None), &none), (Some(1280), 80));
        assert_eq!(resolved(&args(Some(Quality::High), None, None), &none), (Some(1600), 88));
        assert_eq!(resolved(&args(Some(Quality::Max), None, None), &none), (None, 95));
    }

    #[test]
    fn config_preset_applies_when_nothing_else_is_set() {
        assert_eq!(resolved(&ScreenshotArgs::default(), &config(Quality::High, None, None)), (Some(1600), 88));
        assert_eq!(resolved(&ScreenshotArgs::default(), &config(Quality::Max, None, None)), (None, 95));
    }

    #[test]
    fn explicit_config_keys_beat_the_config_preset() {
        let old_style = config(Quality::Max, Some(1024), Some(70));
        assert_eq!(resolved(&ScreenshotArgs::default(), &old_style), (Some(1024), 70));
        let width_only = config(Quality::Low, Some(2000), None);
        assert_eq!(resolved(&ScreenshotArgs::default(), &width_only), (Some(2000), 60));
    }

    #[test]
    fn an_argument_preset_beats_explicit_config_keys() {
        let old_style = config(Quality::Medium, Some(1024), Some(70));
        assert_eq!(resolved(&args(Some(Quality::Low), None, None), &old_style), (Some(960), 60));
        assert_eq!(resolved(&args(Some(Quality::Max), None, None), &old_style), (None, 95));
    }

    #[test]
    fn explicit_arguments_beat_every_preset() {
        let old_style = config(Quality::Low, Some(1024), Some(70));
        assert_eq!(resolved(&args(Some(Quality::Max), Some(800), Some(42)), &old_style), (Some(800), 42));
        assert_eq!(resolved(&args(Some(Quality::Max), None, Some(42)), &old_style), (None, 42));
        assert_eq!(resolved(&args(None, Some(640), None), &old_style), (Some(640), 70));
    }

    #[test]
    fn names_are_the_wire_names() {
        for quality in [Quality::Low, Quality::Medium, Quality::High, Quality::Max] {
            assert_eq!(serde_json::to_value(quality).unwrap(), serde_json::json!(quality.name()));
        }
    }
}
