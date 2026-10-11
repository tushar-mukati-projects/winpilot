use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    matching::{contains_folded, same_process},
    path_guard,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub guard: GuardConfig,
    pub act: ActConfig,
    pub screenshot: ScreenshotConfig,
    pub overlay: OverlayConfig,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScreenshotConfig {
    pub max_width: u32,
    pub quality: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OverlayConfig {
    pub enabled: bool,
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
        let mut deny: Vec<DenyRule> = PASSWORD_MANAGERS
            .iter()
            .map(|process| DenyRule { process: Some((*process).to_string()), title: None })
            .collect();
        deny.push(DenyRule { process: Some("explorer.exe".to_string()), title: Some("recovery".to_string()) });
        deny.push(DenyRule { process: None, title: Some("Downloads\\recovery".to_string()) });
        let deny_paths = std::env::var_os("USERPROFILE")
            .map(|home| vec![PathBuf::from(home).join("Downloads").join("recovery")])
            .unwrap_or_default();
        Self {
            deny,
            deny_paths,
            yield_ms: 600,
            yield_timeout_ms: 5000,
            kill_hotkey: "ctrl+alt+shift+backspace".to_string(),
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
        }
    }
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self { max_width: 1280, quality: 80 }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self { enabled: true }
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
}

impl GuardConfig {
    pub fn denies_window(&self, process: &str, title: &str) -> bool {
        self.deny.iter().any(|rule| rule.matches(process, title))
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
        GuardConfig { deny_paths: vec![PathBuf::from("C:\\Users\\me\\Downloads\\recovery")], ..GuardConfig::default() }
    }

    #[test]
    fn denies_password_managers_by_process() {
        assert!(guard().denies_window("keepassxc.exe", "Passwords"));
        assert!(!guard().denies_window("notepad.exe", "Passwords"));
    }

    #[test]
    fn explorer_rule_needs_both_process_and_title() {
        assert!(guard().denies_window("explorer.exe", "recovery"));
        assert!(!guard().denies_window("explorer.exe", "Documents"));
        assert!(!guard().denies_window("chrome.exe", "recovery tips"));
        assert!(guard().denies_window("notepad.exe", "C:\\Users\\me\\Downloads\\recovery\\keys.txt"));
    }

    #[test]
    fn empty_rule_matches_nothing() {
        let rule = DenyRule { process: None, title: None };
        assert!(!rule.matches("anything.exe", "anything"));
    }

    #[test]
    fn denies_paths_on_component_boundaries() {
        let guard = guard();
        assert!(guard.denies_path(Path::new("C:\\Users\\me\\Downloads\\recovery")));
        assert!(guard.denies_path(Path::new("c:/users/me/downloads/recovery/ssh/id_ed25519")));
        assert!(guard.denies_path(Path::new("\\\\?\\C:\\Users\\me\\Downloads\\recovery\\a.txt")));
        assert!(!guard.denies_path(Path::new("C:\\Users\\me\\Downloads\\recovery-old\\a.txt")));
        assert!(!guard.denies_path(Path::new("C:\\Users\\me\\Documents\\a.txt")));
    }

    #[test]
    fn checked_path_names_the_failed_rule() {
        let guard = guard();
        let denied = guard.checked_path(Path::new("C:\\Users\\me\\Downloads\\recovery\\a.txt")).unwrap_err();
        assert_eq!(denied.code, ErrorCode::Denied);
        let relative = guard.checked_path(Path::new("Downloads\\recovery")).unwrap_err();
        assert_eq!(relative.code, ErrorCode::InvalidArgument);
        assert!(relative.message.contains("is a relative path"), "{}", relative.message);
        assert!(guard.denies_path(Path::new("Downloads\\recovery")));
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
        let rule = DenyRule { process: None, title: Some("Downloads\\recovery".to_string()) };
        assert!(rule.matches("notepad.exe", "C:\\Users\\ME\\DOWNLOADS\\RECOVERY\\keys.txt - Notepad"));
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
    fn partial_toml_style_config_keeps_defaults() {
        let config: Config = serde_json::from_str(r#"{"act":{"settle_max_ms":500}}"#).unwrap();
        assert_eq!(config.act.settle_max_ms, 500);
        assert_eq!(config.act.settle_quiet_ms, ActConfig::default().settle_quiet_ms);
        assert_eq!(config.screenshot, ScreenshotConfig::default());
    }
}
