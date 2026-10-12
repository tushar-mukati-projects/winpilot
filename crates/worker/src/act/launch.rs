use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use deckhand_core::{Error, ErrorCode, Result, WindowInfo, WindowRef, config::GuardConfig, format::window_line};
use deckhand_desktop::apps::{self, AppEntry, AppLaunch};

use super::{
    done::Done,
    foreground::bring_forward,
    launch_args::check_launch_args,
    launch_match::{AppIdentity, Before, Expected, LaunchHints, WindowFacts, is_fresh, pick_launched, switched_to},
    settle,
    shortcut::refuse_shortcut,
};
use crate::{
    app_catalog::{FRESH_LISTING, RECENT_LISTING},
    app_match::{self, without_launch_extension},
    call_scope::CallScope,
    engine::Engine,
    overlay,
    process_tree::{ProcessTable, system_time_now},
    until::until,
    watch::Watch,
    window_classes::is_frame_host,
};

const WINDOW_WAIT: Duration = Duration::from_secs(20);
const LAUNCH_INSTEAD: &str = "launch the program it points to, or the app by name";

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchTarget {
    Path(PathBuf),
    App(AppEntry),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Run { file: PathBuf, args: Option<String> },
    Activate { app: AppEntry, args: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    command: Command,
    label: String,
    expected: Expected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Started {
    label: String,
    expected: Expected,
    since: u64,
}

pub(crate) fn launch(engine: &Engine, app: &str, args: Option<&str>, call: &CallScope) -> Result<Done> {
    let target = launch_target(engine, app.trim())?;
    check_args(engine, args)?;
    let plan = plan(&engine.config.guard, target, args)?;
    engine.guard.before_input(None, call)?;
    let before = before_launch()?;
    let started = start(plan)?;
    let found = until(engine, call, WINDOW_WAIT, &Watch::any(), || launched_window(&before, &started))?;
    let window = found.ok_or_else(|| {
        Error::timeout(format!(
            "launched {} but no window of it appeared within {} s",
            started.label,
            WINDOW_WAIT.as_secs()
        ))
    })?;
    engine.guard.check_window(&window)?;
    let behind = bring_to_front(&window);
    overlay::begin(engine, &window, call);
    settle(engine, &window, call);
    let window = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("launched {}", window_line(&window))).with_note(behind))
}

fn launch_target(engine: &Engine, app: &str) -> Result<LaunchTarget> {
    if !has_separator(app) {
        return find_app(engine, without_launch_extension(app)).map(LaunchTarget::App);
    }
    let path = engine.guard.checked_path(Path::new(app))?;
    refuse_shortcut(&path, LAUNCH_INSTEAD)?;
    if !path.exists() {
        return Err(Error::not_found(format!("{} does not exist", path.display())));
    }
    Ok(LaunchTarget::Path(path))
}

fn check_args(engine: &Engine, args: Option<&str>) -> Result<()> {
    let Some(args) = args.filter(|_| engine.guard.protects_paths()) else { return Ok(()) };
    check_launch_args(args, |path| engine.guard.checked_path(path))
}

fn has_separator(app: &str) -> bool {
    app.contains(['\\', '/'])
}

fn find_app(engine: &Engine, name: &str) -> Result<AppEntry> {
    let apps = engine.apps.apps(FRESH_LISTING)?;
    match app_match::find(&apps, name) {
        Err(error) if error.code == ErrorCode::NotFound => {
            app_match::find(&engine.apps.apps(RECENT_LISTING)?, name).cloned()
        }
        result => result.cloned(),
    }
}

fn plan(guard: &GuardConfig, target: LaunchTarget, args: Option<&str>) -> Result<Plan> {
    let args = args.map(str::to_string);
    match target {
        LaunchTarget::Path(file) => Ok(path_plan(file, args)),
        LaunchTarget::App(app) => match apps::plan_launch(&app, args.as_deref())? {
            AppLaunch::AsListed => Ok(listed_plan(app, args)),
            AppLaunch::Program { exe, args } => program_plan(guard, &app.name, &exe, args),
        },
    }
}

fn path_plan(file: PathBuf, args: Option<String>) -> Plan {
    let name = file.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let expected = expecting(AppIdentity::of_path(&file), LaunchHints::new(&name, Some(&file)));
    Plan { label: file.display().to_string(), expected, command: Command::Run { file, args } }
}

fn listed_plan(app: AppEntry, args: Option<String>) -> Plan {
    let program = apps::program_of(&app);
    let expected = expecting(AppIdentity::of_app(&app), LaunchHints::new(&app.name, program.as_deref()));
    Plan { label: app.name.clone(), expected, command: Command::Activate { app, args } }
}

fn program_plan(guard: &GuardConfig, name: &str, exe: &Path, args: String) -> Result<Plan> {
    let file = guard.checked_path(exe)?;
    if !guard.deny_paths.is_empty() {
        check_launch_args(&args, |path| guard.checked_path(path))?;
    }
    let expected = expecting(AppIdentity::of_path(&file), LaunchHints::new(name, Some(&file)));
    Ok(Plan { label: name.to_string(), expected, command: Command::Run { file, args: Some(args) } })
}

const fn expecting(identity: AppIdentity, hints: LaunchHints) -> Expected {
    Expected { identity, pid: None, hints }
}

fn start(plan: Plan) -> Result<Started> {
    let since = system_time_now();
    let pid = match &plan.command {
        Command::Run { file, args } => apps::launch_path(file, args.as_deref())?,
        Command::Activate { app, args } => apps::launch(app, args.as_deref())?,
    };
    Ok(Started { label: plan.label, expected: Expected { pid, ..plan.expected }, since })
}

fn before_launch() -> Result<Before> {
    let windows = deckhand_desktop::windows()?;
    Ok(Before {
        windows: windows.iter().map(|window| window.hwnd).collect(),
        foreground: deckhand_desktop::foreground_window(),
    })
}

fn launched_window(before: &Before, started: &Started) -> Result<Option<WindowInfo>> {
    let windows = deckhand_desktop::windows()?;
    let candidates: Vec<&WindowInfo> =
        windows.iter().filter(|window| is_fresh(window, before) || switched_to(window, before)).collect();
    let lineage = lineage_for(started, &candidates);
    let facts: Vec<WindowFacts<'_>> =
        candidates.into_iter().map(|window| facts_of(window, started, lineage.as_ref())).collect();
    Ok(pick_launched(&facts, before, &started.expected).cloned())
}

fn lineage_for(started: &Started, candidates: &[&WindowInfo]) -> Option<ProcessTable> {
    let pid = started.expected.pid?;
    candidates.iter().any(|window| window.pid != pid).then(ProcessTable::read)?.ok()
}

fn facts_of<'a>(window: &'a WindowInfo, started: &Started, lineage: Option<&ProcessTable>) -> WindowFacts<'a> {
    let from_launch = match (lineage, started.expected.pid) {
        (Some(table), Some(pid)) => table.descends_from(window.pid, pid, started.since),
        _ => false,
    };
    let base = WindowFacts { window, package: None, content_package: None, from_launch };
    if !started.expected.identity.needs_packages() {
        return base;
    }
    let content = is_frame_host(window).then(|| deckhand_desktop::uwp_content_pid(window.hwnd)).flatten();
    WindowFacts {
        package: deckhand_desktop::package_family(window.pid),
        content_package: content.and_then(deckhand_desktop::package_family),
        ..base
    }
}

fn bring_to_front(window: &WindowInfo) -> Option<String> {
    if !needs_bringing_forward(window, deckhand_desktop::foreground_window()) {
        return None;
    }
    let refused = bring_forward(window).err()?;
    Some(format!("it stayed behind the foreground window ({})", refused.message))
}

fn needs_bringing_forward(window: &WindowInfo, foreground: Option<WindowRef>) -> bool {
    window.on_current_desktop && foreground != Some(window.hwnd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::engine;

    #[test]
    fn paths_are_told_apart_from_names_by_their_separators() {
        assert!(has_separator("C:/Tools/app.exe") && has_separator("tools\\app.exe") && !has_separator("app.exe"));
    }

    #[test]
    fn missing_and_relative_paths_fail_before_launching() {
        let missing = std::env::temp_dir().join("deckhand-no-such-app-7f3a.exe");
        let error = launch_target(engine(), &missing.to_string_lossy()).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(launch_target(engine(), "tools\\app.exe").unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn shortcut_paths_are_refused_before_launching() {
        let shortcut = std::env::temp_dir().join("deckhand-no-such-app-7f3a.lnk");
        let error = launch_target(engine(), &shortcut.to_string_lossy()).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.ends_with(LAUNCH_INSTEAD), "{}", error.message);
    }

    fn guard_denying(folder: &str) -> GuardConfig {
        GuardConfig { deny_paths: vec![PathBuf::from(folder)], ..GuardConfig::default() }
    }

    fn chrome() -> PathBuf {
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    }

    #[test]
    fn a_resolved_program_runs_with_the_combined_args_and_is_matched_by_its_exe() {
        let args = r#"--profile-directory="Default" https://www.youtube.com/@thedeshbhakt/videos"#.to_string();
        let plan = program_plan(&GuardConfig::default(), "Google Chrome", &chrome(), args.clone()).unwrap();
        assert_eq!(plan.label, "Google Chrome");
        assert_eq!(plan.expected.identity, AppIdentity::Exe("chrome".to_string()));
        assert_eq!(
            plan.expected.hints,
            LaunchHints { name: "Google Chrome".to_string(), exe: Some("chrome".to_string()) }
        );
        assert_eq!(plan.command, Command::Run { file: chrome(), args: Some(args) });
    }

    #[test]
    fn a_resolved_program_inside_a_protected_folder_is_refused() {
        let guard = guard_denying(r"C:\Program Files\Google");
        let error = program_plan(&guard, "Google Chrome", &chrome(), "x".to_string()).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
    }

    #[test]
    fn protected_paths_in_the_combined_args_are_refused() {
        let guard = guard_denying(r"C:\Vault");
        let args = r"--profile-directory=Default C:\Vault\keys.txt".to_string();
        let error = program_plan(&guard, "Google Chrome", &chrome(), args).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
    }

    #[test]
    fn paths_and_packaged_apps_launch_as_given() {
        let tool = PathBuf::from(r"C:\Tools\tool.exe");
        let by_path = plan(&GuardConfig::default(), LaunchTarget::Path(tool.clone()), Some("--fast")).unwrap();
        assert_eq!(by_path.command, Command::Run { file: tool, args: Some("--fast".to_string()) });
        let notepad = AppEntry {
            name: "Notepad".to_string(),
            id: "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".to_string(),
            kind: apps::AppKind::Packaged,
        };
        let packaged = plan(&GuardConfig::default(), LaunchTarget::App(notepad.clone()), Some("a.txt")).unwrap();
        assert_eq!(packaged.command, Command::Activate { app: notepad, args: Some("a.txt".to_string()) });
        assert_eq!(packaged.expected.hints, LaunchHints { name: "Notepad".to_string(), exe: None });
    }

    #[test]
    fn a_path_launch_expects_windows_of_its_exe() {
        let tool = PathBuf::from(r"C:\Tools\Tool.exe");
        let planned = path_plan(tool, None);
        assert_eq!(planned.expected.identity, AppIdentity::Exe("tool".to_string()));
        assert_eq!(planned.expected.hints, LaunchHints { name: "Tool".to_string(), exe: Some("tool".to_string()) });
        assert_eq!(planned.expected.pid, None);
    }

    #[test]
    fn a_launched_window_behind_another_is_brought_forward_on_this_desktop_only() {
        let window = crate::test_fixture::window(5, "app.exe", "App", deckhand_core::Rect::new(0, 0, 10, 10));
        assert!(needs_bringing_forward(&window, Some(WindowRef(9))));
        assert!(needs_bringing_forward(&window, None));
        assert!(!needs_bringing_forward(&window, Some(WindowRef(5))));
        let elsewhere = WindowInfo { on_current_desktop: false, desktop: Some(2), ..window };
        assert!(!needs_bringing_forward(&elsewhere, Some(WindowRef(9))));
    }

    #[test]
    fn the_process_table_is_only_read_for_windows_of_other_processes() {
        let started = Started {
            label: "App".to_string(),
            expected: Expected { identity: AppIdentity::Unknown, pid: Some(40), hints: LaunchHints::default() },
            since: 0,
        };
        let own = crate::test_fixture::window(5, "app.exe", "App", deckhand_core::Rect::new(0, 0, 10, 10));
        let own = WindowInfo { pid: 40, ..own };
        assert!(lineage_for(&started, &[&own]).is_none());
        assert!(lineage_for(&started, &[]).is_none());
        let unknown = Started { expected: Expected { pid: None, ..started.expected.clone() }, ..started.clone() };
        assert!(lineage_for(&unknown, &[&own]).is_none());
    }

    #[test]
    fn unknown_names_fail_before_launching() {
        let error = launch_target(engine(), "zzqx-no-such-app-zzqx").unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }
}
