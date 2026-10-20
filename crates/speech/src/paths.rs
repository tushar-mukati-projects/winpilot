use std::path::{Path, PathBuf};

use winpilot_core::{Error, ErrorCode, Result};

const PREFERENCE: [&str; 3] = ["base", "small", "tiny"];

fn model_dir() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::internal("LOCALAPPDATA is not set, so the speech model folder is unknown"))?;
    Ok(models_under(Path::new(&local)))
}

pub fn installed_model_path() -> Result<PathBuf> {
    first_installed(&model_dir()?).ok_or_else(model_missing_error)
}

pub(crate) fn model_missing_error() -> Error {
    Error::new(
        ErrorCode::NeedsUser,
        "no speech model is installed; run scripts\\install.ps1 -SpeechModel base (or small for better accuracy) \
         from the WinPilot repository, which downloads a pinned, hash-checked Whisper model once",
    )
}

fn models_under(local_app_data: &Path) -> PathBuf {
    local_app_data.join("winpilot").join("models")
}

fn model_file_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("ggml-{name}.bin"))
}

fn first_installed(dir: &Path) -> Option<PathBuf> {
    PREFERENCE.iter().map(|name| model_file_in(dir, name)).find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_live_under_local_app_data() {
        let dir = models_under(Path::new(r"C:\Users\someone\AppData\Local"));
        assert_eq!(dir, PathBuf::from(r"C:\Users\someone\AppData\Local\winpilot\models"));
        assert_eq!(model_file_in(&dir, "base"), dir.join("ggml-base.bin"));
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
