use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    agent,
    error::Result,
    matching::{contains_folded, fold, process_stem, same_process},
    path_guard,
    quality::Quality,
    recording::VideoFormat,
    theme::{HexColor, Theme},
};

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
#[serde(deny_unknown_fields)]
pub struct DenyRule {
    #[serde(default)]
    pub process: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "GuardFile")]
pub struct GuardConfig {
    pub deny: Vec<DenyRule>,
    pub deny_paths: Vec<PathBuf>,
    pub yield_ms: u64,
    pub yield_timeout_ms: u64,
    pub kill_hotkey: String,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GuardFile {
    deny: Vec<DenyRule>,
    deny_paths: Vec<PathBuf>,
    yield_ms: u64,
    yield_timeout_ms: u64,
    kill_hotkey: String,
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

const PASSWORD_MANAGERS: [&str; 6] =
    ["KeePass.exe", "KeePassXC.exe", "1Password.exe", "Bitwarden.exe", "Dashlane.exe", "Enpass.exe"];

impl Default for GuardConfig {
    fn default() -> Self {
        let deny = PASSWORD_MANAGERS
            .iter()
            .map(|process| DenyRule { process: Some((*process).to_string()), title: None })
            .collect();
        Self {
            deny,
            deny_paths: Vec::new(),
            yield_ms: 600,
            yield_timeout_ms: 5000,
            kill_hotkey: "ctrl+alt+escape".to_string(),
        }
    }
}

impl Default for GuardFile {
    fn default() -> Self {
        let GuardConfig { yield_ms, yield_timeout_ms, kill_hotkey, .. } = GuardConfig::default();
        Self { deny: Vec::new(), deny_paths: Vec::new(), yield_ms, yield_timeout_ms, kill_hotkey }
    }
}

impl From<GuardFile> for GuardConfig {
    fn from(file: GuardFile) -> Self {
        let built_in = Self::default();
        Self {
            deny: extended(built_in.deny, file.deny),
            deny_paths: extended(built_in.deny_paths, file.deny_paths),
            yield_ms: file.yield_ms,
            yield_timeout_ms: file.yield_timeout_ms,
            kill_hotkey: file.kill_hotkey,
        }
    }
}

fn extended<T: PartialEq>(mut built_in: Vec<T>, extra: Vec<T>) -> Vec<T> {
    for item in extra {
        if !built_in.contains(&item) {
            built_in.push(item);
        }
    }
    built_in
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

impl DenyRule {
    pub fn matches(&self, process: &str, title: &str) -> bool {
        if self.process.is_none() && self.title.is_none() {
            return false;
        }
        let process_matches = self.process.as_deref().is_none_or(|wanted| same_process(wanted, process));
        let title_matches = self.title.as_deref().is_none_or(|wanted| contains_folded(title, wanted));
        process_matches && title_matches
    }

    pub fn mentioned_in(&self, text: &str) -> bool {
        let title = self.title.as_deref().filter(|title| !fold(title).is_empty());
        let process = self.process.as_deref().map(process_stem).filter(|stem| !stem.is_empty());
        title.is_some_and(|title| contains_folded(text, title))
            || process.is_some_and(|stem| fold(text).contains(&stem))
    }
}

impl GuardConfig {
    pub fn denies_window(&self, process: &str, title: &str) -> bool {
        self.deny.iter().any(|rule| rule.matches(process, title))
    }

    pub fn mentions_denied(&self, text: &str) -> bool {
        self.deny.iter().any(|rule| rule.mentioned_in(text))
    }

    pub fn denies_path(&self, path: &Path) -> bool {
        self.checked_path(path).is_err()
    }

    pub fn checked_path(&self, path: &Path) -> Result<PathBuf> {
        path_guard::checked_path(path, &self.deny_paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn guard() -> GuardConfig {
        let mut guard = GuardConfig::default();
        guard.deny.push(DenyRule { process: Some("explorer.exe".to_string()), title: Some("vault".to_string()) });
        guard.deny.push(DenyRule { process: None, title: Some("Documents\\vault".to_string()) });
        guard.deny_paths.push(PathBuf::from("C:\\Users\\me\\Documents\\vault"));
        guard
    }

    #[test]
    fn denies_password_managers_by_process() {
        assert!(guard().denies_window("keepassxc.exe", "Passwords"));
        assert!(!guard().denies_window("notepad.exe", "Passwords"));
    }

    #[test]
    fn built_ins_are_only_the_password_managers() {
        let built_in = GuardConfig::default();
        assert_eq!(built_in.deny.len(), PASSWORD_MANAGERS.len());
        assert!(built_in.deny.iter().all(|rule| rule.title.is_none() && rule.process.is_some()));
        assert!(built_in.deny_paths.is_empty());
        assert!(!built_in.denies_window("explorer.exe", "Downloads"));
    }

    #[test]
    fn process_and_title_rule_needs_both() {
        assert!(guard().denies_window("explorer.exe", "vault"));
        assert!(!guard().denies_window("explorer.exe", "Documents"));
        assert!(!guard().denies_window("chrome.exe", "vault tips"));
        assert!(guard().denies_window("notepad.exe", "C:\\Users\\me\\Documents\\vault\\keys.txt"));
    }

    #[test]
    fn empty_rule_matches_nothing() {
        let rule = DenyRule { process: None, title: None };
        assert!(!rule.matches("anything.exe", "anything"));
        assert!(!rule.mentioned_in("anything"));
    }

    #[test]
    fn text_that_names_a_denied_title_or_process_is_recognised() {
        let guard = guard();
        assert!(guard.mentions_denied("window:\"My  VAULT\" >> role:edit"));
        assert!(guard.mentions_denied("process:keepassxc"));
        assert!(guard.mentions_denied("KeePass Database"));
        assert!(!guard.mentions_denied("window:\"Untitled - Notepad\""));
        assert!(!DenyRule { process: Some(".exe".to_string()), title: Some("  ".to_string()) }.mentioned_in("x.exe"));
    }

    #[test]
    fn denies_paths_on_component_boundaries() {
        let guard = guard();
        assert!(guard.denies_path(Path::new("C:\\Users\\me\\Documents\\vault")));
        assert!(guard.denies_path(Path::new("c:/users/me/documents/vault/ssh/id_ed25519")));
        assert!(guard.denies_path(Path::new("\\\\?\\C:\\Users\\me\\Documents\\vault\\a.txt")));
        assert!(!guard.denies_path(Path::new("C:\\Users\\me\\Documents\\vault-old\\a.txt")));
        assert!(!guard.denies_path(Path::new("C:\\Users\\me\\Documents\\a.txt")));
    }

    #[test]
    fn checked_path_names_the_failed_rule() {
        let guard = guard();
        let denied = guard.checked_path(Path::new("C:\\Users\\me\\Documents\\vault\\a.txt")).unwrap_err();
        assert_eq!(denied.code, ErrorCode::Denied);
        let relative = guard.checked_path(Path::new("Documents\\vault")).unwrap_err();
        assert_eq!(relative.code, ErrorCode::InvalidArgument);
        assert!(relative.message.contains("is a relative path"), "{}", relative.message);
        assert!(guard.denies_path(Path::new("Documents\\vault")));
        let clean = guard.checked_path(Path::new("c:/Users/me/Documents/a.txt")).unwrap();
        assert_eq!(clean, PathBuf::from("C:\\Users\\me\\Documents\\a.txt"));
    }

    #[test]
    fn process_rules_use_the_selector_stem_rule() {
        let rule = DenyRule { process: Some("keepass".to_string()), title: None };
        assert!(rule.matches("KeePass.exe", "x"));
        assert!(rule.matches("KEEPASS", "x"));
        assert!(!rule.matches("KeePassXC.exe", "x"));
        let with_exe = DenyRule { process: Some("Bitwarden.EXE".to_string()), title: None };
        assert!(with_exe.matches("bitwarden", "Vault"));
    }

    #[test]
    fn title_rules_ignore_case_and_spacing() {
        let rule = DenyRule { process: None, title: Some("Documents\\vault".to_string()) };
        assert!(rule.matches("notepad.exe", "C:\\Users\\ME\\DOCUMENTS\\VAULT\\keys.txt - Notepad"));
        assert!(!rule.matches("notepad.exe", "keys.txt - Notepad"));
    }

    #[test]
    fn user_deny_lists_extend_the_built_ins() {
        let config: Config = serde_json::from_str(
            r#"{"guard":{"deny_paths":["D:\\secrets"],"deny":[{"process":"keepass"},{"process":"KeePassXC.exe"}]}}"#,
        )
        .unwrap();
        let built_in = GuardConfig::default();
        assert!(config.guard.denies_window("KeePassXC.exe", "x"));
        assert!(config.guard.denies_window("1Password.exe", "x"));
        assert!(config.guard.denies_window("KeePass.exe", "x"));
        assert_eq!(config.guard.deny.len(), built_in.deny.len() + 1);
        assert_eq!(config.guard.deny_paths.len(), built_in.deny_paths.len() + 1);
        assert_eq!(config.guard.deny_paths.last(), Some(&PathBuf::from("D:\\secrets")));
        assert!(config.guard.deny_paths.starts_with(&built_in.deny_paths));
    }

    #[test]
    fn guard_defaults_survive_partial_and_empty_sections() {
        let empty: Config = serde_json::from_str(r#"{"guard":{}}"#).unwrap();
        assert_eq!(empty.guard, GuardConfig::default());
        let cleared: Config = serde_json::from_str(r#"{"guard":{"deny":[],"deny_paths":[],"yield_ms":50}}"#).unwrap();
        assert_eq!(cleared.guard, GuardConfig { yield_ms: 50, ..GuardConfig::default() });
        assert!(serde_json::from_str::<Config>(r#"{"guard":{"denny":[]}}"#).is_err());
    }

    #[test]
    fn guard_config_round_trips_without_duplicating_built_ins() {
        let mut config = Config::default();
        config.guard.deny.push(DenyRule { process: None, title: Some("Vault".to_string()) });
        config.guard.deny_paths.push(PathBuf::from("D:\\secrets"));
        let json = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }

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
