use std::{
    fs, io,
    path::{Path, PathBuf},
};

use deckhand_core::Config;

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub problem: Option<String>,
}

impl LoadedConfig {
    fn defaults() -> Self {
        Self { config: Config::default(), problem: None }
    }

    fn fallback(problem: String) -> Self {
        Self { config: Config::default(), problem: Some(problem) }
    }
}

pub fn load() -> LoadedConfig {
    let Some(path) = config_path() else { return LoadedConfig::defaults() };
    let loaded = load_from(&path);
    if let Some(problem) = &loaded.problem {
        eprintln!("deckhand: {problem}; using default settings");
    }
    loaded
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("deckhand").join("config.toml"))
}

pub fn load_from(path: &Path) -> LoadedConfig {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedConfig::defaults(),
        Err(error) => LoadedConfig::fallback(format!("could not read {}: {error}", path.display())),
    }
}

pub fn parse(text: &str, path: &Path) -> LoadedConfig {
    match toml::from_str::<Config>(text) {
        Ok(config) => LoadedConfig { config, problem: None },
        Err(error) => LoadedConfig::fallback(describe(&error, text, path)),
    }
}

fn describe(error: &toml::de::Error, text: &str, path: &Path) -> String {
    let message = one_line(error.message());
    match error.span() {
        Some(span) => format!("{} line {}: {message}", path.display(), line_number(text, span.start)),
        None => format!("{}: {message}", path.display()),
    }
}

fn one_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn line_number(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end].split(|byte| *byte == b'\n').count()
}

#[cfg(test)]
mod tests {
    use deckhand_core::config::{ActConfig, LogConfig, ScreenshotConfig};

    use super::*;

    fn path() -> &'static Path {
        Path::new("C:\\cfg\\config.toml")
    }

    #[test]
    fn empty_text_gives_defaults() {
        let loaded = parse("", path());
        assert_eq!(loaded, LoadedConfig::defaults());
    }

    #[test]
    fn valid_config_is_applied() {
        let text = "[log]\nlog_text = true\nretention_days = 3\n\n[screenshot]\nmax_width = 1024\nquality = 70\n";
        let loaded = parse(text, path());
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.log, LogConfig { log_text: true, retention_days: 3 });
        assert_eq!(loaded.config.screenshot, ScreenshotConfig { max_width: 1024, quality: 70 });
    }

    #[test]
    fn partial_config_keeps_other_defaults() {
        let loaded = parse("[act]\nsettle_max_ms = 500\n", path());
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.act.settle_max_ms, 500);
        assert_eq!(loaded.config.act.settle_quiet_ms, ActConfig::default().settle_quiet_ms);
        assert_eq!(loaded.config.log, LogConfig::default());
        assert_eq!(loaded.config.guard, Config::default().guard);
    }

    #[test]
    fn invalid_config_falls_back_to_defaults_with_a_one_line_problem() {
        let loaded = parse("[log]\n\nretention_days = \"soon\"\n", path());
        assert_eq!(loaded.config, Config::default());
        let problem = loaded.problem.unwrap();
        assert!(problem.starts_with("C:\\cfg\\config.toml line 3: "), "{problem}");
        assert!(!problem.contains('\n'));
    }

    #[test]
    fn unknown_keys_are_reported() {
        let loaded = parse("[log]\nlog_txt = true\n", path());
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.problem.unwrap().contains("log_txt"));
    }

    #[test]
    fn broken_syntax_is_reported() {
        let loaded = parse("[log\n", path());
        assert!(loaded.problem.is_some());
    }

    #[test]
    fn missing_file_gives_defaults_without_a_problem() {
        let missing = std::env::temp_dir().join("deckhand-config-that-does-not-exist").join("config.toml");
        let loaded = load_from(&missing);
        assert_eq!(loaded, LoadedConfig::defaults());
    }

    #[test]
    fn line_numbers_count_from_one() {
        assert_eq!(line_number("a\nb\nc", 0), 1);
        assert_eq!(line_number("a\nb\nc", 2), 2);
        assert_eq!(line_number("a\nb\nc", 99), 3);
    }
}
