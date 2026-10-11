use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
#[serde(default, deny_unknown_fields)]
pub struct GuardConfig {
    pub deny: Vec<DenyRule>,
    pub deny_paths: Vec<PathBuf>,
    pub yield_ms: u64,
    pub yield_timeout_ms: u64,
    pub kill_hotkey: String,
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
        let process_matches = self.process.as_deref().is_none_or(|wanted| wanted.eq_ignore_ascii_case(process));
        let title_matches = self.title.as_deref().is_none_or(|wanted| contains_ignore_case(title, wanted));
        process_matches && title_matches
    }
}

impl GuardConfig {
    pub fn denies_window(&self, process: &str, title: &str) -> bool {
        self.deny.iter().any(|rule| rule.matches(process, title))
    }

    pub fn denies_path(&self, path: &Path) -> bool {
        let candidate = normalize_path(path);
        self.deny_paths.iter().map(|denied| normalize_path(denied)).any(|denied| {
            candidate == denied || candidate.strip_prefix(&denied).is_some_and(|rest| rest.starts_with('\\'))
        })
    }
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn normalize_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let text = text.strip_prefix("\\\\?\\").unwrap_or(&text);
    text.trim_end_matches('\\').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn partial_toml_style_config_keeps_defaults() {
        let config: Config = serde_json::from_str(r#"{"act":{"settle_max_ms":500}}"#).unwrap();
        assert_eq!(config.act.settle_max_ms, 500);
        assert_eq!(config.act.settle_quiet_ms, ActConfig::default().settle_quiet_ms);
        assert_eq!(config.screenshot, ScreenshotConfig::default());
    }
}
