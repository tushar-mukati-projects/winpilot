mod guard;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    agent,
    quality::Quality,
    recording::VideoFormat,
    theme::{HexColor, Theme},
};

pub use guard::{DenyRule, GuardConfig};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub guard: GuardConfig,
    pub act: ActConfig,
    pub screenshot: ScreenshotConfig,
    pub record: RecordConfig,
    pub overlay: OverlayConfig,
    pub cursor: CursorConfig,
    pub log: LogConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActConfig {
    pub settle_quiet_ms: u64,
    pub settle_max_ms: u64,
    pub auto_wait_ms: u64,
    pub max_elements: usize,
    pub snapshot_budget_ms: u64,
    pub background: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScreenshotConfig {
    pub preset: Quality,
    pub max_width: Option<u32>,
    pub quality: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecordConfig {
    pub fps: u32,
    pub quality: Quality,
    pub max_seconds: u32,
    pub cursor: bool,
    pub format: VideoFormat,
    pub max_width: Option<u32>,
    pub folder: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OverlayConfig {
    pub enabled: bool,
    pub animations: bool,
    pub capturable: bool,
    pub show_reads: bool,
    pub theme: Theme,
    pub colors: Option<Vec<HexColor>>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CursorConfig {
    pub glide_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    pub log_text: bool,
    pub retention_days: u32,
}

impl Default for ActConfig {
    fn default() -> Self {
        Self {
            settle_quiet_ms: 120,
            settle_max_ms: 1000,
            auto_wait_ms: 2000,
            max_elements: 3000,
            snapshot_budget_ms: 2500,
            background: false,
        }
    }
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self { preset: Quality::Medium, max_width: None, quality: None }
    }
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            quality: Quality::High,
            max_seconds: 300,
            cursor: true,
            format: VideoFormat::Mp4,
            max_width: None,
            folder: None,
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            animations: true,
            capturable: false,
            show_reads: true,
            theme: Theme::Aurora,
            colors: None,
            label: None,
        }
    }
}

impl OverlayConfig {
    pub fn palette(&self) -> Vec<[u8; 3]> {
        let custom = self.colors.as_ref().filter(|colors| !colors.is_empty());
        custom.cloned().unwrap_or_else(|| self.theme.colors()).into_iter().map(|color| color.0).collect()
    }

    pub fn display_label(&self, agent: &str) -> String {
        let configured = self.label.as_deref().map(agent::sanitized_label).filter(|label| !label.is_empty());
        configured.unwrap_or_else(|| agent::agent_label(agent))
    }
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self { glide_ms: 120 }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self { log_text: false, retention_days: 14 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_settings_are_additive_with_defaults() {
        let defaults = Config::default();
        assert!(defaults.overlay.enabled && defaults.overlay.animations && !defaults.overlay.capturable);
        assert!(defaults.overlay.show_reads);
        let quiet: Config = serde_json::from_str(r#"{"overlay":{"show_reads":false}}"#).unwrap();
        assert!(!quiet.overlay.show_reads && quiet.overlay.enabled);
        assert_eq!(defaults.overlay.theme, Theme::Aurora);
        let still: Config = serde_json::from_str(r#"{"overlay":{"animations":false}}"#).unwrap();
        assert_eq!(still.overlay, OverlayConfig { animations: false, ..OverlayConfig::default() });
        let demo: Config = serde_json::from_str(r#"{"overlay":{"enabled":true,"capturable":true}}"#).unwrap();
        assert!(demo.overlay.capturable && demo.overlay.animations);
        let off: Config = serde_json::from_str(r#"{"overlay":{"enabled":false}}"#).unwrap();
        assert!(!off.overlay.enabled && off.overlay.animations);
        assert!(serde_json::from_str::<Config>(r#"{"overlay":{"animation":false}}"#).is_err());
    }

    #[test]
    fn overlay_colours_come_from_the_theme_unless_overridden() {
        let ocean: Config = serde_json::from_str(r#"{"overlay":{"theme":"ocean"}}"#).unwrap();
        assert_eq!(ocean.overlay.palette()[0], [0x0E, 0xA5, 0xE9]);
        let custom: Config =
            serde_json::from_str(r##"{"overlay":{"theme":"ocean","colors":["#FF0000","00ff00"]}}"##).unwrap();
        assert_eq!(custom.overlay.palette(), vec![[255, 0, 0], [0, 255, 0]]);
        let empty: Config = serde_json::from_str(r#"{"overlay":{"colors":[]}}"#).unwrap();
        assert_eq!(empty.overlay.palette().len(), 5);
        let bad = serde_json::from_str::<Config>(r##"{"overlay":{"colors":["#12"]}}"##).unwrap_err().to_string();
        assert!(bad.contains("overlay colour \"#12\" is not a hex colour"), "{bad}");
        assert!(serde_json::from_str::<Config>(r#"{"overlay":{"theme":"neon"}}"#).is_err());
    }

    #[test]
    fn the_overlay_label_overrides_the_agent_name() {
        let config = Config::default();
        assert_eq!(config.overlay.display_label("claude-code"), "Claude");
        assert_eq!(config.overlay.display_label(""), "AI agent");
        let named: Config = serde_json::from_str(r#"{"overlay":{"label":"Helper"}}"#).unwrap();
        assert_eq!(named.overlay.display_label("claude-code"), "Helper");
        let blank: Config = serde_json::from_str(r#"{"overlay":{"label":"  "}}"#).unwrap();
        assert_eq!(blank.overlay.display_label("codex"), "Codex");
        let hidden: Config = serde_json::from_str("{\"overlay\":{\"label\":\"\u{202e}\u{200b}\"}}").unwrap();
        assert_eq!(hidden.overlay.display_label("codex"), "Codex");
        let tricky: Config =
            serde_json::from_str("{\"overlay\":{\"label\":\" My\u{202e}Helper\\n with a very long name \"}}").unwrap();
        assert_eq!(tricky.overlay.display_label("codex"), "MyHelper with a very lo\u{2026}");
    }

    #[test]
    fn cursor_glide_defaults_to_120_ms_and_zero_is_instant() {
        assert_eq!(Config::default().cursor.glide_ms, 120);
        let instant: Config = serde_json::from_str(r#"{"cursor":{"glide_ms":0}}"#).unwrap();
        assert_eq!(instant.cursor, CursorConfig { glide_ms: 0 });
        assert!(serde_json::from_str::<Config>(r#"{"cursor":{"speed":3}}"#).is_err());
    }

    #[test]
    fn partial_toml_style_config_keeps_defaults() {
        let config: Config = serde_json::from_str(r#"{"act":{"settle_max_ms":500}}"#).unwrap();
        assert_eq!(config.act.settle_max_ms, 500);
        assert_eq!(config.act.settle_quiet_ms, ActConfig::default().settle_quiet_ms);
        assert_eq!(config.screenshot, ScreenshotConfig::default());
        assert_eq!(config.record, RecordConfig::default());
    }

    #[test]
    fn background_mode_is_off_unless_configured() {
        assert!(!Config::default().act.background);
        let on: Config = serde_json::from_str(r#"{"act":{"background":true}}"#).unwrap();
        assert!(on.act.background);
        assert_eq!(on.act.settle_max_ms, ActConfig::default().settle_max_ms);
    }

    #[test]
    fn screenshot_keys_stay_optional_beside_the_preset() {
        let old: Config = serde_json::from_str(r#"{"screenshot":{"max_width":1024,"quality":70}}"#).unwrap();
        assert_eq!(
            old.screenshot,
            ScreenshotConfig { preset: Quality::Medium, max_width: Some(1024), quality: Some(70) }
        );
        let preset: Config = serde_json::from_str(r#"{"screenshot":{"preset":"high"}}"#).unwrap();
        assert_eq!(preset.screenshot, ScreenshotConfig { preset: Quality::High, ..ScreenshotConfig::default() });
        assert!(serde_json::from_str::<Config>(r#"{"screenshot":{"preset":"ultra"}}"#).is_err());
    }

    #[test]
    fn record_section_keeps_defaults_and_rejects_typos() {
        let config: Config =
            serde_json::from_str(r#"{"record":{"format":"gif","fps":12,"folder":"D:\\Clips"}}"#).unwrap();
        assert_eq!(config.record.format, VideoFormat::Gif);
        assert_eq!(config.record.fps, 12);
        assert_eq!(config.record.folder, Some(PathBuf::from("D:\\Clips")));
        assert_eq!(config.record.max_seconds, 300);
        assert!(config.record.cursor);
        assert!(serde_json::from_str::<Config>(r#"{"record":{"fsp":12}}"#).is_err());
        let back: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(back, config);
    }
}
