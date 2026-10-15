use winpilot_core::protocol::{Call, Step};

const DISTRO_VARIABLE: &str = "WSL_DISTRO_NAME";
const MOUNT_PREFIX: &str = "/mnt/";

pub fn translated(call: Call) -> Call {
    let distro = std::env::var(DISTRO_VARIABLE).ok().filter(|name| !name.trim().is_empty());
    with_distro(call, distro.as_deref())
}

fn with_distro(call: Call, distro: Option<&str>) -> Call {
    match call {
        Call::Observe(mut args) => {
            args.path = args.path.map(|path| windows_path(&path, distro));
            Call::Observe(args)
        }
        Call::Act(mut args) => {
            args.steps.iter_mut().for_each(|step| translate_step(step, distro));
            Call::Act(args)
        }
    }
}

fn translate_step(step: &mut Step, distro: Option<&str>) {
    match step {
        Step::Pick { paths, .. } => paths.iter_mut().for_each(|path| *path = windows_path(path, distro)),
        Step::Window { app: Some(app), .. } if app.trim_start().starts_with('/') => *app = windows_path(app, distro),
        _ => {}
    }
}

fn windows_path(path: &str, distro: Option<&str>) -> String {
    let trimmed = path.trim();
    if let Some(on_drive) = mounted_drive(trimmed) {
        return on_drive;
    }
    match distro {
        Some(distro) if trimmed.starts_with('/') && !trimmed.starts_with("//") => {
            format!(r"\\wsl.localhost\{distro}{}", trimmed.replace('/', "\\"))
        }
        _ => path.to_string(),
    }
}

fn mounted_drive(path: &str) -> Option<String> {
    let rest = path.strip_prefix(MOUNT_PREFIX)?;
    let mut chars = rest.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    let tail = chars.as_str();
    if !tail.is_empty() && !tail.starts_with('/') {
        return None;
    }
    let tail = if tail.is_empty() { "\\".to_string() } else { tail.replace('/', "\\") };
    Some(format!("{}:{tail}", drive.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use winpilot_core::protocol::{ActArgs, ObserveArgs};

    use super::*;

    fn act(steps: &serde_json::Value) -> Call {
        Call::Act(serde_json::from_value::<ActArgs>(json!({ "steps": steps })).unwrap())
    }

    #[test]
    fn mounted_drives_become_drive_letters() {
        assert_eq!(windows_path("/mnt/c/Users/me/a.mp4", None), r"C:\Users\me\a.mp4");
        assert_eq!(windows_path("/mnt/d", Some("Debian")), r"D:\");
        assert_eq!(windows_path("/mnt/data/x.txt", None), "/mnt/data/x.txt");
    }

    #[test]
    fn linux_paths_go_through_the_distro_share() {
        assert_eq!(
            windows_path("/home/me/work/report.pdf", Some("Debian")),
            r"\\wsl.localhost\Debian\home\me\work\report.pdf"
        );
        assert_eq!(windows_path("/mnt/data/x.txt", Some("Ubuntu")), r"\\wsl.localhost\Ubuntu\mnt\data\x.txt");
    }

    #[test]
    fn windows_paths_and_unknown_distros_are_left_alone() {
        assert_eq!(windows_path(r"C:\Videos\talk.mp4", Some("Debian")), r"C:\Videos\talk.mp4");
        assert_eq!(windows_path("/home/me/a.txt", None), "/home/me/a.txt");
        assert_eq!(windows_path("//server/share/a.txt", Some("Debian")), "//server/share/a.txt");
    }

    #[test]
    fn observe_paths_pick_paths_and_launched_programs_are_translated() {
        let observe = Call::Observe(
            serde_json::from_value::<ObserveArgs>(json!({"mode": "listen", "path": "/mnt/c/a.wav"})).unwrap(),
        );
        let Call::Observe(args) = with_distro(observe, Some("Debian")) else { panic!("expected observe") };
        assert_eq!(args.path.as_deref(), Some(r"C:\a.wav"));
        let steps = json!([
            {"do": "pick", "paths": ["/home/me/a.pdf", r"C:\b.pdf"]},
            {"do": "window", "action": "launch", "app": "/mnt/c/Tools/app.exe"},
            {"do": "window", "action": "launch", "app": "Notepad"}
        ]);
        let Call::Act(args) = with_distro(act(&steps), Some("Debian")) else { panic!("expected act") };
        let Step::Pick { paths, .. } = &args.steps[0] else { panic!("expected pick") };
        assert_eq!(paths, &[r"\\wsl.localhost\Debian\home\me\a.pdf".to_string(), r"C:\b.pdf".to_string()]);
        let Step::Window { app, .. } = &args.steps[1] else { panic!("expected window") };
        assert_eq!(app.as_deref(), Some(r"C:\Tools\app.exe"));
        let Step::Window { app, .. } = &args.steps[2] else { panic!("expected window") };
        assert_eq!(app.as_deref(), Some("Notepad"));
    }
}
