use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Storage::FileSystem::{FILE_READ_DATA, FILE_SHARE_READ, FILE_SHARE_WRITE};

#[derive(Debug)]
pub struct OutputFile {
    path: PathBuf,
    pin: Option<File>,
    kept: AtomicBool,
}

impl OutputFile {
    pub fn create_new(path: &Path) -> io::Result<Self> {
        let pin = OpenOptions::new()
            .write(true)
            .create_new(true)
            .access_mode(FILE_READ_DATA.0)
            .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
            .open(path)?;
        Ok(Self { path: path.to_path_buf(), pin: Some(pin), kept: AtomicBool::new(false) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn keep(&self) {
        self.kept.store(true, Ordering::Release);
    }

    fn is_kept(&self) -> bool {
        self.kept.load(Ordering::Acquire)
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        drop(self.pin.take());
        if !self.is_kept() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, Write};

    use super::*;

    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let folder = std::env::temp_dir().join(format!("winpilot-output-file-{name}-{}", std::process::id()));
            fs::create_dir_all(&folder).unwrap();
            Self(folder)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_new_file_is_pinned_until_dropped_and_removed_unless_kept() {
        let folder = Folder::new("pin");
        let path = folder.0.join("clip.mp4");
        let output = OutputFile::create_new(&path).unwrap();
        assert!(path.exists());
        assert_eq!(OutputFile::create_new(&path).unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert!(fs::remove_file(&path).is_err());
        assert!(fs::rename(&path, folder.0.join("moved.mp4")).is_err());
        drop(output);
        assert!(!path.exists());
    }

    #[test]
    fn writers_can_reopen_the_pinned_file_and_a_kept_file_stays() {
        let folder = Folder::new("keep");
        let path = folder.0.join("clip.gif");
        let output = OutputFile::create_new(&path).unwrap();
        let mut writer = OpenOptions::new().write(true).open(output.path()).unwrap();
        writer.write_all(b"GIF89a").unwrap();
        drop(writer);
        output.keep();
        drop(output);
        assert_eq!(fs::read(&path).unwrap(), b"GIF89a");
    }
}
