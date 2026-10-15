use winpilot_core::{
    Error, Result,
    protocol::{Call, Step},
};

const MOUNT_PREFIX: &str = "/mnt/";

pub fn translated(call: Call) -> Call {
    match call {
        Call::Observe(mut args) => {
            args.path = args.path.map(|path| windows_path(&path));
            Call::Observe(args)
        }
        Call::Act(mut args) => {
            args.steps.iter_mut().for_each(translate_step);
            Call::Act(args)
        }
    }
}

pub fn check(call: &Call) -> Result<()> {
    match linux_paths(call).into_iter().next() {
        Some(path) => Err(linux_only(path)),
        None => Ok(()),
    }
}

fn translate_step(step: &mut Step) {
    match step {
        Step::Pick { paths, .. } => paths.iter_mut().for_each(|path| *path = windows_path(path)),
        Step::Window { app: Some(app), .. } if is_linux(app) => *app = windows_path(app),
        _ => {}
    }
}

fn linux_paths(call: &Call) -> Vec<&str> {
    match call {
        Call::Observe(args) => args.path.as_deref().filter(|path| is_linux(path)).into_iter().collect(),
        Call::Act(args) => args.steps.iter().flat_map(step_linux_paths).collect(),
    }
}

fn step_linux_paths(step: &Step) -> Vec<&str> {
    match step {
        Step::Pick { paths, .. } => paths.iter().map(String::as_str).filter(|path| is_linux(path)).collect(),
        Step::Window { app: Some(app), .. } if is_linux(app) => vec![app.as_str()],
        _ => Vec::new(),
    }
}

fn is_linux(path: &str) -> bool {
    let trimmed = path.trim_start();
    trimmed.starts_with('/') && !trimmed.starts_with("//")
}

fn linux_only(path: &str) -> Error {
    Error::invalid(format!(
        "{} is a Linux path, and WinPilot runs on Windows: copy the file under /mnt/c (for example /mnt/c/Users/<you>/Downloads/) and pass that path",
        path.trim()
    ))
}

fn windows_path(path: &str) -> String {
    mounted_drive(path.trim()).unwrap_or_else(|| path.to_string())
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
    use winpilot_core::{
        ErrorCode,
        protocol::{ActArgs, ObserveArgs},
    };

    use super::*;

    fn act(steps: &serde_json::Value) -> Call {
        Call::Act(serde_json::from_value::<ActArgs>(json!({ "steps": steps })).unwrap())
    }

    fn listen(path: &str) -> Call {
        Call::Observe(serde_json::from_value::<ObserveArgs>(json!({"mode": "listen", "path": path})).unwrap())
    }

    #[test]
    fn mounted_drives_become_drive_letters() {
        assert_eq!(windows_path("/mnt/c/Users/me/a.mp4"), r"C:\Users\me\a.mp4");
        assert_eq!(windows_path("/mnt/d"), r"D:\");
        assert_eq!(windows_path("/mnt/data/x.txt"), "/mnt/data/x.txt");
        assert_eq!(windows_path(r"C:\Videos\talk.mp4"), r"C:\Videos\talk.mp4");
    }

    #[test]
    fn observe_paths_pick_paths_and_launched_programs_are_translated() {
        let Call::Observe(args) = translated(listen("/mnt/c/a.wav")) else { panic!("expected observe") };
        assert_eq!(args.path.as_deref(), Some(r"C:\a.wav"));
        let steps = json!([
            {"do": "pick", "paths": ["/mnt/c/Users/me/a.pdf", r"C:\b.pdf"]},
            {"do": "window", "action": "launch", "app": "/mnt/c/Tools/app.exe"},
            {"do": "window", "action": "launch", "app": "Notepad"}
        ]);
        let Call::Act(args) = translated(act(&steps)) else { panic!("expected act") };
        let Step::Pick { paths, .. } = &args.steps[0] else { panic!("expected pick") };
        assert_eq!(paths, &[r"C:\Users\me\a.pdf".to_string(), r"C:\b.pdf".to_string()]);
        let Step::Window { app, .. } = &args.steps[1] else { panic!("expected window") };
        assert_eq!(app.as_deref(), Some(r"C:\Tools\app.exe"));
        let Step::Window { app, .. } = &args.steps[2] else { panic!("expected window") };
        assert_eq!(app.as_deref(), Some("Notepad"));
    }

    #[test]
    fn linux_only_paths_are_refused_with_a_way_out() {
        let error = check(&translated(listen("/home/me/talk.wav"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.starts_with("/home/me/talk.wav is a Linux path"), "{}", error.message);
        assert!(error.message.contains("/mnt/c"), "{}", error.message);
        let pick = act(&json!([{"do": "pick", "paths": ["/mnt/c/a.pdf", "/tmp/b.pdf"]}]));
        assert!(check(&translated(pick)).unwrap_err().message.starts_with("/tmp/b.pdf"));
        assert!(check(&translated(listen("/mnt/c/a.wav"))).is_ok());
        assert!(check(&translated(listen("//server/share/a.wav"))).is_ok());
    }
}
