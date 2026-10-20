use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    matching::{contains_folded, fold, process_stem, same_process},
    path_guard,
};

const AGENT_DECK: &str = "agent-deck";
const DEFAULT_YIELD_MS: u64 = 600;
const DEFAULT_YIELD_TIMEOUT_MS: u64 = 5000;

const PASSWORD_MANAGERS: [&str; 6] =
    ["KeePass.exe", "KeePassXC.exe", "1Password.exe", "Bitwarden.exe", "Dashlane.exe", "Enpass.exe"];

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
    pub partial_hosts: Vec<String>,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GuardFile {
    deny: Vec<DenyRule>,
    deny_paths: Vec<PathBuf>,
    yield_ms: u64,
    yield_timeout_ms: u64,
    kill_hotkey: String,
    partial_hosts: Vec<String>,
}

impl Default for GuardConfig {
    fn default() -> Self {
        let deny = PASSWORD_MANAGERS
            .iter()
            .map(|process| DenyRule { process: Some((*process).to_string()), title: None })
            .collect();
        Self {
            deny,
            deny_paths: Vec::new(),
            yield_ms: DEFAULT_YIELD_MS,
            yield_timeout_ms: DEFAULT_YIELD_TIMEOUT_MS,
            kill_hotkey: "ctrl+alt+escape".to_string(),
            partial_hosts: vec![AGENT_DECK.to_string()],
        }
    }
}

impl Default for GuardFile {
    fn default() -> Self {
        let GuardConfig { yield_ms, yield_timeout_ms, kill_hotkey, partial_hosts, .. } = GuardConfig::default();
        Self { deny: Vec::new(), deny_paths: Vec::new(), yield_ms, yield_timeout_ms, kill_hotkey, partial_hosts }
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
            partial_hosts: file.partial_hosts,
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

impl DenyRule {
    pub fn matches(&self, process: &str, title: &str) -> bool {
        if self.process.is_none() && self.title.is_none() {
            return false;
        }
        let process_matches = self.process.as_deref().is_none_or(|wanted| same_process(wanted, process));
        let title_matches = self.title.as_deref().is_none_or(|wanted| contains_folded(title, wanted));
        process_matches && title_matches
    }

    fn mentioned_in(&self, text: &str) -> bool {
        let title = self.title.as_deref().filter(|title| !fold(title).is_empty());
        let process = self.process.as_deref().map(process_stem).filter(|stem| !stem.is_empty());
        title.is_some_and(|title| contains_folded(text, title))
            || process.is_some_and(|stem| fold(text).contains(&stem))
    }
}

impl GuardConfig {
    pub fn allows_partial_host(&self, process: &str) -> bool {
        self.partial_hosts.iter().any(|host| !host.trim().is_empty() && contains_folded(process, host))
    }

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
    use crate::{config::Config, error::ErrorCode};
    #[test]
    fn partial_hosts_default_to_agent_deck_and_can_be_turned_off() {
        let guard = GuardConfig::default();
        assert!(guard.allows_partial_host("agent-deck.exe"));
        assert!(guard.allows_partial_host("Agent-Deck.corrupt.exe"));
        assert!(!guard.allows_partial_host("WindowsTerminal.exe"));
        let off: Config = serde_json::from_str(r#"{"guard":{"partial_hosts":[]}}"#).unwrap();
        assert!(!off.guard.allows_partial_host("agent-deck.exe"));
        let blank: Config = serde_json::from_str(r#"{"guard":{"partial_hosts":[" "]}}"#).unwrap();
        assert!(!blank.guard.allows_partial_host("agent-deck.exe"));
    }

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
}
