use std::path::{Path, PathBuf};

use winpilot_core::{Error, Result};

use super::dialog_kind::DialogKind;
use crate::act::shortcut::refuse_shortcut;

const PICK_INSTEAD: &str = "pick the target file itself, not a shortcut";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) files: Vec<String>,
    folder: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Requested {
    pub(crate) name: String,
    pub(crate) existed: bool,
}

impl Selection {
    pub(crate) fn parse(paths: &[String], check: impl Fn(&Path) -> Result<PathBuf>) -> Result<Self> {
        if paths.is_empty() {
            return Err(Error::invalid("pick needs at least one path in \"paths\""));
        }
        let parsed = paths.iter().map(|path| file_path(path, &check)).collect::<Result<Vec<_>>>()?;
        let folder = shared_folder(&parsed)?;
        let files = parsed.iter().map(|path| path.to_string_lossy().into_owned()).collect();
        Ok(Self { files, folder })
    }

    pub(crate) fn folder_text(&self) -> String {
        self.folder.to_string_lossy().into_owned()
    }

    pub(crate) fn file_names(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|file| Path::new(file).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
    }

    pub(crate) fn quoted_names(&self) -> String {
        self.file_names().iter().map(|name| format!("\"{name}\"")).collect::<Vec<_>>().join(" ")
    }

    pub(crate) fn requested(&self) -> Requested {
        let name = self.file_names().into_iter().next().unwrap_or_default();
        let existed = self.files.first().is_some_and(|file| Path::new(file).is_file());
        Requested { name, existed }
    }

    pub(crate) fn check_folder(&self) -> Result<()> {
        if !self.folder.is_dir() {
            return Err(Error::not_found(format!("folder {} does not exist", self.folder.display())));
        }
        if self.files.len() > 1 {
            return self.files.iter().try_for_each(|file| existing_file(file));
        }
        Ok(())
    }

    pub(crate) fn check_for(&self, kind: DialogKind) -> Result<()> {
        match kind {
            DialogKind::Open => self.files.iter().try_for_each(|file| existing_file(file)),
            DialogKind::Save if self.files.len() > 1 => Err(Error::invalid("a Save dialog takes exactly one path")),
            DialogKind::Save => Ok(()),
        }
    }
}

fn file_path(text: &str, check: impl Fn(&Path) -> Result<PathBuf>) -> Result<PathBuf> {
    let text = text.trim();
    if text.ends_with(['\\', '/']) {
        return Err(Error::invalid(format!("'{text}' names a folder; pick picks files")));
    }
    let path = check(Path::new(text))?;
    if path.file_name().is_none() {
        return Err(Error::invalid(format!("'{text}' names a folder; pick picks files")));
    }
    refuse_shortcut(&path, PICK_INSTEAD)?;
    Ok(path)
}

fn shared_folder(paths: &[PathBuf]) -> Result<PathBuf> {
    let folders: Vec<&Path> = paths.iter().filter_map(|path| path.parent()).collect();
    let Some((first, rest)) = folders.split_first() else {
        return Err(Error::invalid("pick paths need a folder"));
    };
    if folders.len() != paths.len() || rest.iter().any(|folder| !same_folder(folder, first)) {
        return Err(Error::invalid("several files must share one folder; pick from one folder per call"));
    }
    Ok(first.to_path_buf())
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let fold = |path: &Path| path.to_string_lossy().trim_end_matches('\\').to_lowercase();
    fold(a) == fold(b)
}

fn existing_file(file: &str) -> Result<()> {
    let path = Path::new(file);
    if path.is_dir() {
        return Err(Error::invalid(format!("{file} is a folder; pick picks files")));
    }
    if !path.is_file() {
        return Err(Error::not_found(format!("{file} does not exist")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use winpilot_core::{ErrorCode, config::GuardConfig};

    use super::*;
    use crate::test_fixture::{TempFiles, temp_file};

    fn guard() -> GuardConfig {
        GuardConfig { deny_paths: vec![PathBuf::from("C:\\Denied")], ..GuardConfig::default() }
    }

    fn parse(paths: &[&str]) -> Result<Selection> {
        let paths: Vec<String> = paths.iter().map(|path| (*path).to_string()).collect();
        Selection::parse(&paths, |path| guard().checked_path(path))
    }

    fn parse_error(paths: &[&str]) -> ErrorCode {
        parse(paths).unwrap_err().code
    }

    fn one(path: &Path) -> Selection {
        parse(&[path.to_string_lossy().as_ref()]).unwrap()
    }

    #[test]
    fn paths_are_cleaned_and_keep_their_order() {
        let selection = parse(&[" C:/Data/b.txt ", "c:\\data\\a.txt"]).unwrap();
        assert_eq!(selection.files, ["C:\\Data\\b.txt", "C:\\data\\a.txt"]);
        assert_eq!(selection.folder_text(), "C:\\Data");
        assert_eq!(selection.quoted_names(), "\"b.txt\" \"a.txt\"");
    }

    #[test]
    fn rejects_bad_path_lists() {
        assert_eq!(parse_error(&[]), ErrorCode::InvalidArgument);
        assert_eq!(parse_error(&["report.txt"]), ErrorCode::InvalidArgument);
        assert_eq!(parse_error(&["C:\\Data\\"]), ErrorCode::InvalidArgument);
        assert_eq!(parse_error(&["C:\\"]), ErrorCode::InvalidArgument);
        assert_eq!(parse_error(&["C:\\Data\\..\\a.txt"]), ErrorCode::InvalidArgument);
        assert_eq!(parse_error(&["C:\\Data\\a.txt", "C:\\Other\\b.txt"]), ErrorCode::InvalidArgument);
    }

    #[test]
    fn protected_folders_are_denied() {
        assert_eq!(parse_error(&["C:\\Denied\\keys.txt"]), ErrorCode::Denied);
    }

    #[test]
    fn shortcuts_are_refused() {
        for path in ["C:\\Data\\Report.lnk", "C:\\Data\\site.URL", "C:\\Data\\Documents.library-ms"] {
            let error = parse(&[path]).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument, "{path}");
            assert!(error.message.ends_with("pick the target file itself, not a shortcut"), "{}", error.message);
        }
        assert!(parse(&["C:\\Data\\link.txt"]).is_ok());
    }

    #[test]
    fn open_needs_existing_files() {
        let file = temp_file("open.txt");
        let _cleanup = TempFiles(vec![file.clone()]);
        let present = one(&file);
        assert!(present.check_folder().is_ok() && present.check_for(DialogKind::Open).is_ok());
        let missing = one(&file.with_extension("missing"));
        assert!(missing.check_folder().is_ok());
        assert_eq!(missing.check_for(DialogKind::Open).unwrap_err().code, ErrorCode::NotFound);
        let temp = std::env::temp_dir();
        let folder = parse(&[temp.to_string_lossy().trim_end_matches('\\')]).unwrap();
        assert_eq!(folder.check_for(DialogKind::Open).unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn save_takes_one_path_in_an_existing_folder() {
        let single = one(&std::env::temp_dir().join("winpilot-pick-new-file.txt"));
        assert!(single.check_folder().is_ok() && single.check_for(DialogKind::Save).is_ok());
        let pair = parse(&["C:\\Data\\a.txt", "C:\\Data\\b.txt"]).unwrap();
        assert_eq!(pair.check_for(DialogKind::Save).unwrap_err().code, ErrorCode::InvalidArgument);
        let nowhere = parse(&["C:\\winpilot-no-such-folder-7f3a\\a.txt"]).unwrap();
        assert_eq!(nowhere.check_folder().unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn several_files_must_all_exist_up_front() {
        let first = temp_file("one.txt");
        let second = temp_file("two.txt");
        let _cleanup = TempFiles(vec![first.clone(), second.clone()]);
        let both = parse(&[first.to_string_lossy().as_ref(), second.to_string_lossy().as_ref()]);
        assert!(both.unwrap().check_folder().is_ok());
        let gap = parse(&[first.to_string_lossy().as_ref(), first.with_extension("gone").to_string_lossy().as_ref()]);
        assert_eq!(gap.unwrap().check_folder().unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn the_request_records_the_name_and_whether_it_existed() {
        let file = temp_file("exists.txt");
        let _cleanup = TempFiles(vec![file.clone()]);
        let requested = one(&file).requested();
        assert!(requested.existed);
        assert!(requested.name.ends_with("exists.txt"));
        assert!(!one(&file.with_extension("new")).requested().existed);
    }
}
