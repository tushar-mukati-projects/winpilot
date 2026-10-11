use std::path::PathBuf;

use deckhand_core::config::{Config, DenyRule, GuardConfig};

use crate::enumwin::WinInfo;

const CONFIG_RELATIVE: [&str; 2] = ["deckhand", "config.toml"];

pub struct PrivacyGuard {
    guard: GuardConfig,
    origin: String,
}

impl PrivacyGuard {
    pub fn load(args: &[String]) -> Result<Self, String> {
        let (mut guard, origin) = match config_path() {
            Some(path) if path.exists() => (read_guard(&path)?, format!("the deny list in {}", path.display())),
            _ => (GuardConfig::default(), "Deckhand's built-in deny list (no config file found)".to_string()),
        };
        let extra = extra_titles(args);
        let origin = if extra.is_empty() { origin } else { format!("{origin} plus {} --deny-title", extra.len()) };
        guard.deny.extend(extra.into_iter().map(|title| DenyRule { process: None, title: Some(title) }));
        Ok(Self { guard, origin })
    }

    pub fn denies(&self, window: &WinInfo) -> bool {
        self.guard.denies_window(&window.process, &window.title)
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }
}

fn config_path() -> Option<PathBuf> {
    let app_data = PathBuf::from(std::env::var_os("APPDATA")?);
    app_data.is_absolute().then(|| CONFIG_RELATIVE.iter().fold(app_data, |path, part| path.join(part)))
}

fn read_guard(path: &PathBuf) -> Result<GuardConfig, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let config: Config = toml::from_str(&text).map_err(|error| {
        format!("{} is invalid, so the privacy guard cannot use it: {}", path.display(), error.message())
    })?;
    Ok(config.guard)
}

fn extra_titles(args: &[String]) -> Vec<String> {
    args.windows(2)
        .filter(|pair| pair[0] == "--deny-title")
        .map(|pair| pair[1].trim().to_string())
        .filter(|title| !title.is_empty())
        .collect()
}
