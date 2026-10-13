use std::path::{Path, PathBuf};

use winpilot_core::{Error, ErrorCode, Result};

pub const MODEL_NAMES: [&str; 3] = ["tiny", "base", "small"];
pub const DEFAULT_MODEL: &str = "base";
const PREFERENCE: [&str; 3] = ["base", "small", "tiny"];

pub fn model_dir() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::internal("LOCALAPPDATA is not set, so the speech model folder is unknown"))?;
    Ok(models_under(Path::new(&local)))
}

pub fn model_path(name: &str) -> Result<PathBuf> {
    model_file_in(&model_dir()?, name)
}

pub fn default_model_path() -> Result<PathBuf> {
    model_path(DEFAULT_MODEL)
}

pub fn installed_model_path() -> Result<PathBuf> {
    first_installed(&model_dir()?).ok_or_else(model_missing_error)
}

pub fn model_missing_error() -> Error {
    Error::new(
        ErrorCode::NeedsUser,
        "no speech model is installed; run scripts\\install.ps1 -SpeechModel base (or small for better accuracy) \
         from the WinPilot repository, which downloads a pinned, hash-checked Whisper model once",
    )
}

fn models_under(local_app_data: &Path) -> PathBuf {
    local_app_data.join("winpilot").join("models")
}

fn model_file_in(dir: &Path, name: &str) -> Result<PathBuf> {
    if !MODEL_NAMES.contains(&name) {
        return Err(Error::invalid(format!("unknown speech model '{name}'; use one of {}", MODEL_NAMES.join(", "))));
    }
    Ok(dir.join(format!("ggml-{name}.bin")))
}

fn first_installed(dir: &Path) -> Option<PathBuf> {
    PREFERENCE.iter().filter_map(|name| model_file_in(dir, name).ok()).find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_live_under_local_app_data() {
        let dir = models_under(Path::new(r"C:\Users\someone\AppData\Local"));
        assert_eq!(dir, PathBuf::from(r"C:\Users\someone\AppData\Local\winpilot\models"));
        assert_eq!(model_file_in(&dir, "base").unwrap(), dir.join("ggml-base.bin"));
    }

    #[test]
    fn unknown_model_names_are_invalid() {
        let error = model_file_in(Path::new("x"), "large").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(model_file_in(Path::new("x"), "../base").is_err());
    }

    #[test]
    fn default_model_is_base_and_listed() {
        assert!(MODEL_NAMES.contains(&DEFAULT_MODEL));
        assert!(PREFERENCE.iter().all(|name| MODEL_NAMES.contains(name)));
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        assert_eq!(first_installed(Path::new(r"Z:\winpilot-speech-test-missing")), None);
    }

    #[test]
    fn missing_model_error_names_the_install_command() {
        let error = model_missing_error();
        assert_eq!(error.code, ErrorCode::NeedsUser);
        assert!(error.message.contains("install.ps1 -SpeechModel base"));
    }
}
