use std::path::Path;

use deckhand_core::Result;

use crate::{
    apps_folder::list_apps_folder,
    com::ComApartment,
    shell_launch::{activate_packaged, shell_execute},
};

const APPS_FOLDER_PREFIX: &str = "shell:AppsFolder\\";
const OPEN_VERB: &str = "open";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    Packaged,
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    pub id: String,
    pub kind: AppKind,
}

impl AppKind {
    pub(crate) fn of_id(id: &str) -> Self {
        if id.contains('!') { Self::Packaged } else { Self::Desktop }
    }
}

pub fn installed_apps() -> Result<Vec<AppEntry>> {
    let _com = ComApartment::enter_mta()?;
    list_apps_folder()
}

pub fn launch(app: &AppEntry, args: Option<&str>) -> Result<Option<u32>> {
    let _com = ComApartment::enter_mta()?;
    match app.kind {
        AppKind::Packaged => activate_packaged(&app.id, args).map(Some),
        AppKind::Desktop => shell_execute(&format!("{APPS_FOLDER_PREFIX}{}", app.id), None, args),
    }
}

pub fn launch_path(path: &Path, args: Option<&str>) -> Result<Option<u32>> {
    let _com = ComApartment::enter_mta()?;
    shell_execute(&path.to_string_lossy(), Some(OPEN_VERB), args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_follows_aumid_shape() {
        assert_eq!(AppKind::of_id("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"), AppKind::Packaged);
        assert_eq!(AppKind::of_id("Microsoft.Windows.Explorer"), AppKind::Desktop);
    }

    #[test]
    fn installed_apps_lists_familiar_entries() {
        let apps = installed_apps().unwrap();
        assert!(apps.len() > 10, "only {} apps", apps.len());
        assert!(apps.iter().any(|app| app.name.contains("Notepad") || app.name.contains("Settings")));
        assert!(apps.iter().all(|app| !app.id.is_empty() && !app.name.is_empty()));
        assert!(apps.windows(2).all(|pair| pair[0].name.to_lowercase() <= pair[1].name.to_lowercase()));
        let mut ids: Vec<&str> = apps.iter().map(|app| app.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), apps.len());
    }

    #[test]
    #[ignore = "launches Notepad"]
    fn launch_notepad() {
        let apps = installed_apps().unwrap();
        let notepad = apps.iter().find(|app| app.name == "Notepad").unwrap();
        launch(notepad, None).unwrap();
    }

    #[test]
    #[ignore = "launches Notepad by path"]
    fn launch_notepad_by_path() {
        launch_path(Path::new("C:\\Windows\\System32\\notepad.exe"), None).unwrap();
    }
}
