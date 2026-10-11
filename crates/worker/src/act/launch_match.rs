use std::{collections::HashSet, path::Path};

use deckhand_core::{WindowInfo, WindowRef};
use deckhand_desktop::apps::{AppEntry, AppKind};

use crate::window_classes::POPUP_MENU;

const MIN_WORD: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppIdentity {
    Exe(String),
    Package { family: String, words: Vec<String> },
    Words(Vec<String>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFacts<'a> {
    pub window: &'a WindowInfo,
    pub package: Option<String>,
    pub content_package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Before {
    pub windows: HashSet<WindowRef>,
    pub foreground: Option<WindowRef>,
}

impl AppIdentity {
    pub fn of_path(path: &Path) -> Self {
        let is_exe = path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
        match path.file_stem().filter(|_| is_exe) {
            Some(stem) => Self::Exe(stem.to_string_lossy().to_lowercase()),
            None => Self::Unknown,
        }
    }

    pub fn of_app(app: &AppEntry) -> Self {
        match app.kind {
            AppKind::Packaged => {
                let family = app.id.split('!').next().unwrap_or_default().to_lowercase();
                Self::Package { family, words: words(&app.name) }
            }
            AppKind::Desktop => match exe_stem(&app.id) {
                Some(stem) => Self::Exe(stem),
                None => Self::Words([words(&app.name), words(&app.id)].concat()),
            },
        }
    }

    pub fn needs_packages(&self) -> bool {
        matches!(self, Self::Package { .. })
    }
}

pub fn launched_by(facts: &WindowFacts<'_>, identity: &AppIdentity, pid: Option<u32>) -> bool {
    if pid == Some(facts.window.pid) {
        return true;
    }
    let stem = process_stem(&facts.window.process);
    match identity {
        AppIdentity::Exe(exe) => stem == *exe,
        AppIdentity::Package { family, words } => {
            same_family(facts.package.as_deref(), family)
                || same_family(facts.content_package.as_deref(), family)
                || words.contains(&stem)
        }
        AppIdentity::Words(words) => words.contains(&stem),
        AppIdentity::Unknown => false,
    }
}

pub fn pick_launched<'a>(
    candidates: &[WindowFacts<'a>],
    before: &Before,
    identity: &AppIdentity,
    pid: Option<u32>,
) -> Option<&'a WindowInfo> {
    let ours = |facts: &&WindowFacts<'a>| launched_by(facts, identity, pid);
    let fresh = candidates.iter().filter(|facts| is_fresh(facts.window, before)).find(ours);
    let switched = || candidates.iter().filter(|facts| switched_to(facts.window, before)).find(ours);
    fresh.or_else(switched).map(|facts| facts.window)
}

pub fn is_fresh(window: &WindowInfo, before: &Before) -> bool {
    !before.windows.contains(&window.hwnd) && window.class_name != POPUP_MENU
}

pub fn switched_to(window: &WindowInfo, before: &Before) -> bool {
    window.foreground && before.foreground != Some(window.hwnd) && before.windows.contains(&window.hwnd)
}

fn same_family(found: Option<&str>, wanted: &str) -> bool {
    found.is_some_and(|found| !wanted.is_empty() && found.eq_ignore_ascii_case(wanted))
}

fn exe_stem(id: &str) -> Option<String> {
    let file = id.rsplit(['\\', '/']).next()?;
    let lower = file.to_lowercase();
    lower.strip_suffix(".exe").filter(|stem| !stem.is_empty()).map(str::to_string)
}

fn process_stem(process: &str) -> String {
    let lower = process.trim().to_lowercase();
    if let Some(stem) = lower.strip_suffix(".exe") {
        return stem.to_string();
    }
    lower
}

fn words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_WORD)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn app_window(hwnd: i64, pid: u32, process: &str) -> WindowInfo {
        WindowInfo {
            pid,
            class_name: "AppWindow".to_string(),
            ..window(hwnd, process, "App", Rect::new(0, 0, 100, 100))
        }
    }

    fn facts(window: &WindowInfo) -> WindowFacts<'_> {
        WindowFacts { window, package: None, content_package: None }
    }

    fn before(hwnds: &[i64], foreground: Option<i64>) -> Before {
        Before { windows: hwnds.iter().copied().map(WindowRef).collect(), foreground: foreground.map(WindowRef) }
    }

    fn app(name: &str, id: &str, kind: AppKind) -> AppEntry {
        AppEntry { name: name.to_string(), id: id.to_string(), kind }
    }

    fn picked(windows: &[WindowInfo], before: &Before, identity: &AppIdentity, pid: Option<u32>) -> Option<i64> {
        let candidates: Vec<WindowFacts<'_>> = windows.iter().map(facts).collect();
        pick_launched(&candidates, before, identity, pid).map(|window| window.hwnd.0)
    }

    #[test]
    fn identities_come_from_paths_and_app_entries() {
        assert_eq!(
            AppIdentity::of_path(&PathBuf::from("C:\\Tools\\Fixture.EXE")),
            AppIdentity::Exe("fixture".to_string())
        );
        assert_eq!(AppIdentity::of_path(&PathBuf::from("C:\\Tools\\run.bat")), AppIdentity::Unknown);
        let notepad_pp = app("Notepad++", "{6D809377}\\Notepad++\\notepad++.exe", AppKind::Desktop);
        assert_eq!(AppIdentity::of_app(&notepad_pp), AppIdentity::Exe("notepad++".to_string()));
        let code = app("Visual Studio Code", "Microsoft.VisualStudioCode", AppKind::Desktop);
        let AppIdentity::Words(words) = AppIdentity::of_app(&code) else { panic!("expected words") };
        assert!(words.contains(&"code".to_string()));
        let calculator = app("Calculator", "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", AppKind::Packaged);
        assert_eq!(
            AppIdentity::of_app(&calculator),
            AppIdentity::Package {
                family: "microsoft.windowscalculator_8wekyb3d8bbwe".to_string(),
                words: vec!["calculator".to_string()]
            }
        );
    }

    #[test]
    fn a_new_window_of_the_launched_exe_wins_over_strangers() {
        let mut stranger = app_window(2, 20, "other.exe");
        stranger.foreground = true;
        let windows = [stranger, app_window(3, 30, "app.exe")];
        let exe = AppIdentity::Exe("app".to_string());
        assert_eq!(picked(&windows, &before(&[], None), &exe, Some(99)), Some(3));
        assert_eq!(picked(&windows[..1], &before(&[], None), &exe, Some(99)), None);
    }

    #[test]
    fn the_returned_pid_is_always_accepted() {
        let windows = [app_window(2, 20, "launcher-child.exe")];
        assert_eq!(picked(&windows, &before(&[], None), &AppIdentity::Unknown, Some(20)), Some(2));
        assert_eq!(picked(&windows, &before(&[], None), &AppIdentity::Unknown, None), None);
    }

    #[test]
    fn packaged_apps_match_by_family_or_content_family_or_name() {
        let identity =
            AppIdentity::Package { family: "contoso.app_abc".to_string(), words: vec!["contoso".to_string()] };
        let frame = app_window(5, 77, "ApplicationFrameHost.exe");
        let hosted =
            WindowFacts { window: &frame, package: None, content_package: Some("Contoso.App_abc".to_string()) };
        assert!(launched_by(&hosted, &identity, None));
        assert!(!launched_by(&facts(&frame), &identity, None));
        let own = app_window(6, 78, "Worker.exe");
        let packaged =
            WindowFacts { window: &own, package: Some("contoso.app_abc".to_string()), content_package: None };
        assert!(launched_by(&packaged, &identity, None));
        assert!(launched_by(&facts(&app_window(7, 79, "Contoso.exe")), &identity, None));
    }

    #[test]
    fn only_a_foreground_switch_to_the_same_exe_counts() {
        let mut existing = app_window(1, 10, "settings.exe");
        existing.foreground = true;
        let windows = [existing];
        let settings = AppIdentity::Exe("settings".to_string());
        assert_eq!(picked(&windows, &before(&[1, 2], Some(2)), &settings, None), Some(1));
        assert_eq!(picked(&windows, &before(&[1, 2], Some(1)), &settings, None), None);
        let other = AppIdentity::Exe("paint".to_string());
        assert_eq!(picked(&windows, &before(&[1, 2], Some(2)), &other, None), None);
    }

    #[test]
    fn popup_menus_are_not_launched_windows() {
        let mut menu = app_window(4, 30, "app.exe");
        menu.class_name = POPUP_MENU.to_string();
        assert_eq!(picked(&[menu], &before(&[], None), &AppIdentity::Exe("app".to_string()), Some(30)), None);
    }
}
