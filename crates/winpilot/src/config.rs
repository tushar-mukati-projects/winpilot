use std::{
    fs, io,
    path::{Path, PathBuf},
};

use winpilot_core::{Config, Error};

use crate::app_data;

const HOW_TO_FIX: &str = "fix it or delete it to use the defaults";

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub problem: Option<String>,
}

impl LoadedConfig {
    fn defaults() -> Self {
        Self { config: Config::default(), problem: None }
    }

    fn invalid(problem: String) -> Self {
        Self { config: Config::default(), problem: Some(problem) }
    }
}

pub fn load() -> LoadedConfig {
    let Some(path) = config_path() else { return LoadedConfig::defaults() };
    let loaded = load_from(&path);
    if let Some(problem) = &loaded.problem {
        eprintln!("winpilot: {}; every tool except status is refused until then", advice(problem));
    }
    loaded
}

pub fn advice(problem: &str) -> String {
    format!("{problem}; {HOW_TO_FIX}")
}

pub fn refusal(problem: &str) -> Error {
    Error::invalid(advice(problem))
}

fn config_path() -> Option<PathBuf> {
    app_data::dir_from_env("APPDATA").map(|dir| dir.join("winpilot").join("config.toml"))
}

fn load_from(path: &Path) -> LoadedConfig {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedConfig::defaults(),
        Err(error) => LoadedConfig::invalid(format!("{} could not be read ({error})", path.display())),
    }
}

fn parse(text: &str, path: &Path) -> LoadedConfig {
    match toml::from_str::<Config>(text) {
        Ok(config) => LoadedConfig { config, problem: None },
        Err(error) => LoadedConfig::invalid(describe(&error, text, path)),
    }
}

fn describe(error: &toml::de::Error, text: &str, path: &Path) -> String {
    let message = one_line(error.message());
    match error.span() {
        Some(span) => format!("{} is invalid (line {}: {message})", path.display(), line_number(text, span.start)),
        None => format!("{} is invalid ({message})", path.display()),
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
    use winpilot_core::config::{ActConfig, LogConfig, ScreenshotConfig};

    use super::*;
    use crate::testing::TempDir;

    fn path() -> &'static Path {
        Path::new("C:\\cfg\\config.toml")
    }

    #[test]
    fn a_private_folder_is_guarded_by_user_rules() {
        let text = "[guard]\n\
                    deny = [{ process = \"explorer.exe\", title = \"vault\" }, { title = \"Documents\\\\vault\" }]\n\
                    deny_paths = ['C:\\Users\\me\\Documents\\vault']\n";
        let loaded = parse(text, path());
        assert_eq!(loaded.problem, None);
        let guard = &loaded.config.guard;
        assert!(guard.denies_path(Path::new("C:\\Users\\me\\Documents\\vault\\keys.txt")));
        assert!(guard.denies_window("explorer.exe", "vault"));
        assert!(guard.denies_window("notepad.exe", "C:\\Users\\me\\Documents\\vault\\keys.txt - Notepad"));
        assert!(guard.denies_window("KeePassXC.exe", "Passwords"));
        assert!(!guard.denies_window("notepad.exe", "Untitled - Notepad"));
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
        let expected = ScreenshotConfig { max_width: Some(1024), quality: Some(70), ..ScreenshotConfig::default() };
        assert_eq!(loaded.config.screenshot, expected);
    }

    #[test]
    fn presets_and_the_record_section_parse_from_toml() {
        let text = "[screenshot]\npreset = \"high\"\n\n[record]\nformat = \"gif\"\nfps = 12\nquality = \"medium\"\nmax_seconds = 60\ncursor = false\nmax_width = 800\nfolder = 'D:\\Clips'\n";
        let loaded = parse(text, path());
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.screenshot.preset, winpilot_core::protocol::Quality::High);
        let record = &loaded.config.record;
        assert_eq!(record.format, winpilot_core::protocol::VideoFormat::Gif);
        assert_eq!((record.fps, record.max_seconds, record.cursor, record.max_width), (12, 60, false, Some(800)));
        assert_eq!(record.quality, winpilot_core::protocol::Quality::Medium);
        assert_eq!(record.folder.as_deref(), Some(Path::new("D:\\Clips")));
        let typo = parse("[record]\nfrmat = \"gif\"\n", path());
        assert!(typo.problem.is_some());
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
    fn invalid_config_is_a_one_line_problem_naming_the_line() {
        let loaded = parse("[log]\n\nretention_days = \"soon\"\n", path());
        let problem = loaded.problem.unwrap();
        assert!(problem.starts_with("C:\\cfg\\config.toml is invalid (line 3: "), "{problem}");
        assert!(problem.ends_with(')'), "{problem}");
        assert!(!problem.contains('\n'));
    }

    #[test]
    fn refusal_tells_the_user_how_to_fix_it() {
        let error = refusal("C:\\cfg\\config.toml is invalid (line 3: bad)");
        assert_eq!(error.code, winpilot_core::ErrorCode::InvalidArgument);
        assert_eq!(
            error.message,
            "C:\\cfg\\config.toml is invalid (line 3: bad); fix it or delete it to use the defaults"
        );
    }

    #[test]
    fn unreadable_config_is_a_problem() {
        let temp = TempDir::new("config-unreadable");
        let problem = load_from(temp.path()).problem.unwrap();
        assert!(problem.contains("could not be read"), "{problem}");
    }

    #[test]
    fn unknown_keys_are_reported() {
        let loaded = parse("[log]\nlog_txt = true\n", path());
        assert!(loaded.problem.unwrap().contains("log_txt"));
    }

    #[test]
    fn broken_syntax_is_reported() {
        let loaded = parse("[log\n", path());
        assert!(loaded.problem.is_some());
    }

    #[test]
    fn missing_file_gives_defaults_without_a_problem() {
        let missing = std::env::temp_dir().join("winpilot-config-that-does-not-exist").join("config.toml");
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
