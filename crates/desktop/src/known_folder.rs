use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use deckhand_core::{Context, Result};
use windows::Win32::UI::Shell::{FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath};

use crate::co_task_string::CoTaskString;

pub fn videos_folder() -> Result<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_Videos, KF_FLAG_DEFAULT, None) }
        .context("finding the Videos folder")?;
    let path = CoTaskString(path);
    Ok(PathBuf::from(OsString::from_wide(unsafe { path.0.as_wide() })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_videos_folder_is_an_absolute_path() {
        let folder = videos_folder().unwrap();
        assert!(folder.is_absolute(), "{}", folder.display());
    }
}
