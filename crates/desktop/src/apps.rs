use std::path::{Path, PathBuf};

use deckhand_core::{Error, Result};

use crate::{
    activation::Activation,
    app_shortcut::Shortcut,
    apps_folder::list_apps_folder,
    com::ComApartment,
    shell_launch::{ShellRequest, activate_packaged, shell_execute},
};

pub(crate) const APPS_FOLDER_PREFIX: &str = "shell:AppsFolder\\";
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppLaunch {
    AsListed,
    Program { exe: PathBuf, args: String },
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

pub fn plan_launch(app: &AppEntry, args: Option<&str>) -> Result<AppLaunch> {
    let Some(args) = args.filter(|args| !args.trim().is_empty()) else { return Ok(AppLaunch::AsListed) };
    if app.kind == AppKind::Packaged {
        return Ok(AppLaunch::AsListed);
    }
    let _com = ComApartment::enter_any()?;
    program_launch(app, &Shortcut::of_app(&app.id, &app.name)?, args)
}

pub fn program_of(app: &AppEntry) -> Option<PathBuf> {
    if app.kind == AppKind::Packaged {
        return None;
    }
    let _com = ComApartment::enter_any().ok()?;
    Shortcut::of_app(&app.id, &app.name).ok()?.program()
}

pub fn launch(app: &AppEntry, args: Option<&str>, activation: Activation) -> Result<Option<u32>> {
    let _com = ComApartment::enter_any()?;
    match app.kind {
        AppKind::Packaged => activate_packaged(&app.id, args).map(Some),
        AppKind::Desktop => {
            let file = format!("{APPS_FOLDER_PREFIX}{}", app.id);
            shell_execute(&ShellRequest { file: &file, verb: None, args, activation })
        }
    }
}

pub fn launch_path(path: &Path, args: Option<&str>, activation: Activation) -> Result<Option<u32>> {
    let _com = ComApartment::enter_any()?;
    let file = path.to_string_lossy();
    shell_execute(&ShellRequest { file: &file, verb: Some(OPEN_VERB), args, activation })
}

fn program_launch(app: &AppEntry, shortcut: &Shortcut, args: &str) -> Result<AppLaunch> {
    let exe = shortcut.program().ok_or_else(|| takes_no_args(app))?;
    Ok(AppLaunch::Program { exe, args: shortcut.command_line(args) })
}

fn takes_no_args(app: &AppEntry) -> Error {
    Error::unsupported(format!(
        "{} can't be given args: its Start menu entry doesn't start a program file; launch the program by its full path instead",
        app.name
    ))
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

    fn app(name: &str, id: &str) -> AppEntry {
        AppEntry { name: name.to_string(), id: id.to_string(), kind: AppKind::of_id(id) }
    }

    fn chrome_entry(arguments: Option<&str>) -> Shortcut {
        Shortcut {
            target: Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe".to_string()),
            arguments: arguments.map(str::to_string),
        }
    }

    #[test]
    fn launches_without_args_go_through_the_start_menu_entry() {
        let chrome = app("Google Chrome", "Chrome");
        assert_eq!(plan_launch(&chrome, None).unwrap(), AppLaunch::AsListed);
        assert_eq!(plan_launch(&chrome, Some("  ")).unwrap(), AppLaunch::AsListed);
    }

    #[test]
    fn packaged_apps_have_no_program_file_to_name() {
        assert_eq!(program_of(&app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App")), None);
    }

    #[test]
    fn packaged_apps_take_args_through_activation() {
        let notepad = app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App");
        assert_eq!(plan_launch(&notepad, Some(r"C:\notes.txt")).unwrap(), AppLaunch::AsListed);
    }

    #[test]
    fn desktop_apps_with_args_run_their_program_after_the_entrys_own_arguments() {
        let work = app("Work - Chrome", "Chrome.UserData.Profile2");
        let planned = program_launch(&work, &chrome_entry(Some(r#"--profile-directory="Profile 2""#)), "https://a.b/c");
        assert_eq!(
            planned.unwrap(),
            AppLaunch::Program {
                exe: PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
                args: r#"--profile-directory="Profile 2" https://a.b/c"#.to_string(),
            }
        );
    }

    #[test]
    fn desktop_entries_that_start_no_program_refuse_args() {
        let explorer = app("File Explorer", "Microsoft.Windows.Explorer");
        let shell_place =
            Shortcut { target: Some("::{52205FD8-5DFB-447D-801A-D0B52F2E83E1}".to_string()), arguments: None };
        let error = program_launch(&explorer, &shell_place, r"C:\Users").unwrap_err();
        assert_eq!(error.code, deckhand_core::ErrorCode::Unsupported);
        assert!(error.message.starts_with("File Explorer can't be given args"), "{}", error.message);
        assert!(program_launch(&explorer, &Shortcut::default(), r"C:\Users").is_err());
    }

    #[test]
    fn every_desktop_entry_plans_a_program_or_refuses_args() {
        let apps = installed_apps().unwrap();
        for app in apps.iter().filter(|app| app.kind == AppKind::Desktop) {
            match plan_launch(app, Some("--probe")) {
                Ok(AppLaunch::Program { exe, args }) => {
                    assert!(exe.is_absolute(), "{}", exe.display());
                    assert!(args.ends_with("--probe"), "{args}");
                }
                Ok(AppLaunch::AsListed) => panic!("{} would drop its args", app.name),
                Err(error) => assert_eq!(error.code, deckhand_core::ErrorCode::Unsupported, "{}", error.message),
            }
        }
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
            launch(calculator, None, Activation::Activate).unwrap();
        });
    }

    #[test]
    #[ignore = "opens and closes the fixture app"]
    fn launches_an_exe_by_path_with_arguments() {
        let Some(exe) = deckhand_fixture::fixture_exe() else { return };
        let title = format!("Deckhand Fixture launch {}", std::process::id());
        let args = format!("--title \"{title}\" --pos 120,120");
        launch_and_close(&title, || {
            launch_path(&exe, Some(&args), Activation::Activate).unwrap();
        });
    }
}
