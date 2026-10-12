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
    let _com = ComApartment::enter_any()?;
    list_apps_folder()
}

pub fn launch(app: &AppEntry, args: Option<&str>) -> Result<Option<u32>> {
    let _com = ComApartment::enter_any()?;
    match app.kind {
        AppKind::Packaged => activate_packaged(&app.id, args).map(Some),
        AppKind::Desktop => shell_execute(&format!("{APPS_FOLDER_PREFIX}{}", app.id), None, args),
    }
}

pub fn launch_path(path: &Path, args: Option<&str>) -> Result<Option<u32>> {
    let _com = ComApartment::enter_any()?;
    shell_execute(&path.to_string_lossy(), Some(OPEN_VERB), args)
}

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use deckhand_core::WindowRef;

    use super::*;
    use crate::{close_window, windows};

    const CALCULATOR: &str = "Calculator";
    const WINDOW_WAIT: Duration = Duration::from_secs(10);

    fn titled(title: &str) -> Vec<WindowRef> {
        windows().unwrap().into_iter().filter(|window| window.title == title).map(|window| window.hwnd).collect()
    }

    fn shown_titled(title: &str) -> Vec<WindowRef> {
        let deadline = Instant::now() + WINDOW_WAIT;
        loop {
            let shown = titled(title);
            if !shown.is_empty() || Instant::now() >= deadline {
                return shown;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn launch_and_close(title: &str, launch_it: impl FnOnce()) {
        if !titled(title).is_empty() {
            return;
        }
        launch_it();
        let shown = shown_titled(title);
        for hwnd in &shown {
            close_window(*hwnd).unwrap();
        }
        assert!(!shown.is_empty(), "{title} never showed a window");
    }

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
    #[ignore = "opens and closes Calculator"]
    fn launches_a_packaged_app() {
        let apps = installed_apps().unwrap();
        let Some(calculator) = apps.iter().find(|app| app.name == CALCULATOR) else { return };
        launch_and_close(CALCULATOR, || {
            launch(calculator, None).unwrap();
        });
    }

    #[test]
    #[ignore = "opens and closes the fixture app"]
    fn launches_an_exe_by_path_with_arguments() {
        let Some(exe) = deckhand_fixture::fixture_exe() else { return };
        let title = format!("Deckhand Fixture launch {}", std::process::id());
        let args = format!("--title \"{title}\" --pos 120,120");
        launch_and_close(&title, || {
            launch_path(&exe, Some(&args)).unwrap();
        });
    }
}
