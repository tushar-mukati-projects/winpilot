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
    pub from_launch: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchHints {
    pub name: String,
    pub exe: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub identity: AppIdentity,
    pub pid: Option<u32>,
    pub hints: LaunchHints,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Before {
    pub windows: HashSet<WindowRef>,
    pub foreground: Option<WindowRef>,
}

impl LaunchHints {
    pub fn new(name: &str, program: Option<&Path>) -> Self {
        let exe = program.and_then(Path::file_stem).map(|stem| stem.to_string_lossy().to_lowercase());
        Self { name: name.to_string(), exe }
    }
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
    if pid == Some(facts.window.pid) || facts.from_launch {
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

pub fn looks_launched(window: &WindowInfo, hints: &LaunchHints) -> bool {
    let stem = process_stem(&window.process);
    let named = words(&hints.name);
    let titled = words(&window.title);
    hints.exe.as_deref() == Some(stem.as_str())
        || named.contains(&stem)
        || (!named.is_empty() && named.iter().all(|word| titled.contains(word)))
}

pub fn pick_launched<'a>(
    candidates: &[WindowFacts<'a>],
    before: &Before,
    expected: &Expected,
) -> Option<&'a WindowInfo> {
    let ours = |facts: &&WindowFacts<'a>| launched_by(facts, &expected.identity, expected.pid);
    let fresh = || candidates.iter().filter(|facts| is_fresh(facts.window, before));
    let switched = || candidates.iter().filter(|facts| switched_to(facts.window, before)).find(ours);
    let named = || fresh().find(|facts| looks_launched(facts.window, &expected.hints));
    fresh().find(ours).or_else(switched).or_else(named).map(|facts| facts.window)
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
        WindowFacts { window, package: None, content_package: None, from_launch: false }
    }

    fn before(hwnds: &[i64], foreground: Option<i64>) -> Before {
        Before { windows: hwnds.iter().copied().map(WindowRef).collect(), foreground: foreground.map(WindowRef) }
    }

    fn app(name: &str, id: &str, kind: AppKind) -> AppEntry {
        AppEntry { name: name.to_string(), id: id.to_string(), kind }
    }

    fn expected(identity: &AppIdentity, pid: Option<u32>) -> Expected {
        Expected { identity: identity.clone(), pid, hints: LaunchHints::default() }
    }

    fn picked(windows: &[WindowInfo], before: &Before, identity: &AppIdentity, pid: Option<u32>) -> Option<i64> {
        let candidates: Vec<WindowFacts<'_>> = windows.iter().map(facts).collect();
        pick_launched(&candidates, before, &expected(identity, pid)).map(|window| window.hwnd.0)
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
        let hosted = WindowFacts { content_package: Some("Contoso.App_abc".to_string()), ..facts(&frame) };
        assert!(launched_by(&hosted, &identity, None));
        assert!(!launched_by(&facts(&frame), &identity, None));
        let own = app_window(6, 78, "Worker.exe");
        let packaged = WindowFacts { package: Some("contoso.app_abc".to_string()), ..facts(&own) };
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

    fn titled(hwnd: i64, pid: u32, process: &str, title: &str) -> WindowInfo {
        WindowInfo { title: title.to_string(), ..app_window(hwnd, pid, process) }
    }

    fn capcut_hints() -> LaunchHints {
        LaunchHints::new("CapCut", Some(&PathBuf::from(r"C:\Users\me\AppData\Local\CapCut\Apps\CapCut.exe")))
    }

    #[test]
    fn hints_take_the_program_stem_in_lower_case() {
        assert_eq!(capcut_hints(), LaunchHints { name: "CapCut".to_string(), exe: Some("capcut".to_string()) });
        assert_eq!(LaunchHints::new("Files", None).exe, None);
    }

    #[test]
    fn a_window_of_a_process_the_launch_started_is_ours_whatever_its_exe() {
        let window = app_window(9, 90, "Worker.exe");
        let spawned = WindowFacts { from_launch: true, ..facts(&window) };
        assert!(launched_by(&spawned, &AppIdentity::Exe("launcher".to_string()), Some(80)));
        assert!(!launched_by(&facts(&window), &AppIdentity::Exe("launcher".to_string()), Some(80)));
    }

    #[test]
    fn windows_are_recognised_by_program_name_or_title() {
        let hints = capcut_hints();
        assert!(looks_launched(&titled(1, 10, "CapCut.exe", "Untitled"), &hints));
        assert!(looks_launched(&titled(2, 11, "VEEditor.exe", "CapCut"), &hints));
        assert!(!looks_launched(&titled(3, 12, "notepad.exe", "Cap notes"), &hints));
        let chrome = LaunchHints::new("Google Chrome", None);
        assert!(looks_launched(&titled(4, 13, "browser.exe", "YouTube - Google Chrome"), &chrome));
        assert!(looks_launched(&titled(5, 14, "chrome.exe", "New Tab"), &chrome));
        assert!(!looks_launched(&titled(6, 15, "browser.exe", "Chrome flags"), &chrome));
        assert!(!looks_launched(&titled(7, 16, "app.exe", "Anything"), &LaunchHints::default()));
    }

    #[test]
    fn a_new_window_named_after_the_app_is_the_last_resort() {
        let windows = [titled(2, 20, "VEEditor.exe", "CapCut"), titled(3, 30, "other.exe", "Other")];
        let wrong_identity = AppIdentity::Exe("c:.users.me.capcut.apps.8.7.capcut".to_string());
        let mut wanted = expected(&wrong_identity, Some(99));
        let candidates: Vec<WindowFacts<'_>> = windows.iter().map(facts).collect();
        assert_eq!(pick_launched(&candidates, &before(&[], None), &wanted), None);
        wanted.hints = capcut_hints();
        assert_eq!(pick_launched(&candidates, &before(&[], None), &wanted).map(|window| window.hwnd.0), Some(2));
    }

    #[test]
    fn launched_processes_win_over_windows_that_only_look_right() {
        let lookalike = titled(2, 20, "capcut.exe", "CapCut");
        let spawned = titled(3, 30, "VEEditor.exe", "Editor");
        let candidates = [facts(&lookalike), WindowFacts { from_launch: true, ..facts(&spawned) }];
        let wanted = Expected { identity: AppIdentity::Unknown, pid: Some(99), hints: capcut_hints() };
        assert_eq!(pick_launched(&candidates, &before(&[], None), &wanted).map(|window| window.hwnd.0), Some(3));
    }

    #[test]
    fn windows_that_existed_before_never_match_by_name() {
        let old = titled(2, 20, "capcut.exe", "CapCut");
        let wanted = Expected { identity: AppIdentity::Unknown, pid: None, hints: capcut_hints() };
        assert_eq!(pick_launched(&[facts(&old)], &before(&[2], None), &wanted), None);
    }

    #[test]
    fn popup_menus_are_not_launched_windows() {
        let mut menu = app_window(4, 30, "app.exe");
        menu.class_name = POPUP_MENU.to_string();
        assert_eq!(picked(&[menu], &before(&[], None), &AppIdentity::Exe("app".to_string()), Some(30)), None);
    }
}
