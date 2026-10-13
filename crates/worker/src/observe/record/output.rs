use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use chrono::Local;
use winpilot_capture::OutputFile;
use winpilot_core::{Context, Error, Result, config::RecordConfig, protocol::VideoFormat};

use super::naming::{candidates, default_stem};
use crate::guard::Guard;

const RECORDINGS_SUBFOLDER: &str = "WinPilot";

pub(crate) fn reserve(
    guard: &Guard,
    config: &RecordConfig,
    requested: Option<&str>,
    format: VideoFormat,
) -> Result<OutputFile> {
    if let Some(text) = requested {
        return reserve_exact(&guard.checked_path(Path::new(text.trim()))?);
    }
    let folder = guard.checked_path(&recordings_folder(config)?)?;
    reserve_in(&folder, &default_stem(Local::now().naive_local()), format)
}

fn reserve_exact(path: &Path) -> Result<OutputFile> {
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(Error::invalid(format!("the folder for {} does not exist", path.display())));
    }
    OutputFile::create_new(path).map_err(|error| match error.kind() {
        ErrorKind::AlreadyExists => {
            Error::invalid(format!("{} already exists; record never overwrites a file", path.display()))
        }
        _ => Error::internal(format!("creating {}: {error}", path.display())),
    })
}

fn reserve_in(folder: &Path, stem: &str, format: VideoFormat) -> Result<OutputFile> {
    fs::create_dir_all(folder).context("creating the recordings folder")?;
    for name in candidates(stem, format.extension()) {
        let path = folder.join(name);
        match OutputFile::create_new(&path) {
            Ok(reserved) => return Ok(reserved),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(Error::internal(format!("creating {}: {error}", path.display()))),
        }
    }
    Err(Error::internal(format!("every recording name for {stem} in {} is taken", folder.display())))
}

fn recordings_folder(config: &RecordConfig) -> Result<PathBuf> {
    if let Some(folder) = &config.folder {
        return Ok(folder.clone());
    }
    Ok(winpilot_desktop::videos_folder()?.join(RECORDINGS_SUBFOLDER))
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    struct TempFolder(PathBuf);

    impl TempFolder {
        fn new(name: &str) -> Self {
            let folder = std::env::temp_dir().join(format!("winpilot-output-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&folder);
            Self(folder)
        }
    }

    impl Drop for TempFolder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn file_name(path: &Path) -> String {
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn default_names_never_collide() {
        let temp = TempFolder::new("collide");
        let first = reserve_in(&temp.0, "winpilot-1", VideoFormat::Mp4).unwrap();
        let second = reserve_in(&temp.0, "winpilot-1", VideoFormat::Mp4).unwrap();
        let third = reserve_in(&temp.0, "winpilot-1", VideoFormat::Mp4).unwrap();
        let gif = reserve_in(&temp.0, "winpilot-1", VideoFormat::Gif).unwrap();
        let names = [&first, &second, &third, &gif].map(|reserved| file_name(reserved.path()));
        assert_eq!(names, ["winpilot-1.mp4", "winpilot-1-2.mp4", "winpilot-1-3.mp4", "winpilot-1.gif"]);
    }

    #[test]
    fn an_unkept_reservation_is_removed_and_a_kept_one_stays() {
        let temp = TempFolder::new("keep");
        let dropped = reserve_in(&temp.0, "clip", VideoFormat::Gif).unwrap();
        let dropped_path = dropped.path().to_path_buf();
        assert!(dropped_path.exists());
        drop(dropped);
        assert!(!dropped_path.exists());
        let kept = reserve_in(&temp.0, "clip", VideoFormat::Gif).unwrap();
        kept.keep();
        let kept_path = kept.path().to_path_buf();
        drop(kept);
        assert!(kept_path.exists());
    }

    #[test]
    fn an_exact_path_must_be_new_and_in_an_existing_folder() {
        let temp = TempFolder::new("exact");
        let missing_folder = reserve_exact(&temp.0.join("clip.mp4")).err().unwrap();
        assert_eq!(missing_folder.code, ErrorCode::InvalidArgument);
        assert!(missing_folder.message.contains("does not exist"), "{}", missing_folder.message);
        fs::create_dir_all(&temp.0).unwrap();
        let path = temp.0.join("clip.mp4");
        let kept = reserve_exact(&path).unwrap();
        kept.keep();
        drop(kept);
        let taken = reserve_exact(&path).err().unwrap();
        assert_eq!(taken.code, ErrorCode::InvalidArgument);
        assert!(taken.message.ends_with("already exists; record never overwrites a file"), "{}", taken.message);
    }

    #[test]
    fn the_configured_folder_wins_over_the_videos_folder() {
        let config = RecordConfig { folder: Some(PathBuf::from("D:\\Clips")), ..RecordConfig::default() };
        assert_eq!(recordings_folder(&config).unwrap(), PathBuf::from("D:\\Clips"));
        let default = recordings_folder(&RecordConfig::default()).unwrap();
        assert_eq!(default, winpilot_desktop::videos_folder().unwrap().join("WinPilot"));
    }
}
