use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{Error, ErrorCode, Result};

const VERBATIM_PREFIX: &str = r"\\?\";
const FORBIDDEN_CHARS: [char; 7] = ['<', '>', '"', '|', '?', '*', ':'];

pub fn checked_path(path: &Path, protected: &[PathBuf]) -> Result<PathBuf> {
    let clean = plain_absolute(path)?;
    if protected.iter().any(|root| is_inside(&clean, root)) {
        let message = format!("'{}' is inside a folder Deckhand is configured to protect", clean.display());
        return Err(Error::new(ErrorCode::Denied, message));
    }
    Ok(clean)
}

fn plain_absolute(path: &Path) -> Result<PathBuf> {
    let text = path.to_str().ok_or_else(|| rejected(&path.to_string_lossy(), "is not valid Unicode"))?;
    check_prefix(text)?;
    check_expansions(text)?;
    let (drive, rest) = split_drive(text)?;
    check_characters(text, rest)?;
    let parts = components(text, rest)?;
    Ok(PathBuf::from(format!("{}:\\{}", drive.to_ascii_uppercase(), parts.join("\\"))))
}

fn check_prefix(text: &str) -> Result<()> {
    if text.is_empty() {
        return Err(Error::invalid("empty path; pass a full path like C:\\folder\\file.txt"));
    }
    let mut leading = text.chars();
    if leading.next().is_some_and(is_separator) && leading.next().is_some_and(is_separator) {
        return Err(rejected(text, "is a UNC or device path (\\\\server\\…, \\\\?\\…, \\\\.\\…)"));
    }
    Ok(())
}

fn check_expansions(text: &str) -> Result<()> {
    if text.matches('%').count() >= 2 {
        return Err(rejected(text, "contains %…%, and environment variables are not expanded"));
    }
    if text.contains('~') {
        return Err(rejected(text, "contains '~'; home shortcuts and 8.3 short names are not allowed"));
    }
    Ok(())
}

fn split_drive(text: &str) -> Result<(char, &str)> {
    let mut chars = text.chars();
    let (Some(drive), Some(colon)) = (chars.next(), chars.next()) else {
        return Err(rejected(text, "is a relative path"));
    };
    if !drive.is_ascii_alphabetic() || colon != ':' {
        let reason = if is_separator(drive) { "is relative to the current drive" } else { "is a relative path" };
        return Err(rejected(text, reason));
    }
    let rest = chars.as_str();
    match rest.strip_prefix(is_separator) {
        Some(after_root) => Ok((drive, after_root)),
        None => Err(rejected(text, "is relative to the current folder of its drive")),
    }
}

fn check_characters(text: &str, rest: &str) -> Result<()> {
    match rest.chars().find(|&ch| FORBIDDEN_CHARS.contains(&ch) || ch.is_control()) {
        Some(':') => {
            Err(rejected(text, "contains ':' after the drive letter (alternate data streams are not allowed)"))
        }
        Some(ch) => Err(rejected(text, &format!("contains {ch:?}, which is not allowed in a file path"))),
        None => Ok(()),
    }
}

fn components<'a>(text: &str, rest: &'a str) -> Result<Vec<&'a str>> {
    let rest = rest.strip_suffix(is_separator).unwrap_or(rest);
    if rest.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = rest.split(is_separator).collect();
    for part in &parts {
        check_component(text, part)?;
    }
    Ok(parts)
}

fn check_component(text: &str, part: &str) -> Result<()> {
    if part.is_empty() {
        return Err(rejected(text, "contains a doubled separator"));
    }
    if part == "." || part == ".." {
        return Err(rejected(text, "contains a '.' or '..' component"));
    }
    if part.ends_with(['.', ' ']) {
        return Err(rejected(text, "has a name ending in '.' or a space, which Windows silently strips"));
    }
    Ok(())
}

fn is_separator(ch: char) -> bool {
    ch == '\\' || ch == '/'
}

fn rejected(text: &str, reason: &str) -> Error {
    Error::invalid(format!("path '{text}' {reason}; pass a plain absolute path like C:\\folder\\file.txt"))
}

fn is_inside(candidate: &Path, root: &Path) -> bool {
    lexically_inside(candidate, root) || lexically_inside(&resolved(candidate), &resolved(root))
}

fn lexically_inside(candidate: &Path, root: &Path) -> bool {
    let root = comparable(root);
    !root.is_empty() && comparable(candidate).starts_with(&root)
}

fn comparable(path: &Path) -> Vec<String> {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(VERBATIM_PREFIX).unwrap_or(&text);
    text.split(is_separator).filter(|part| !part.is_empty()).map(str::to_lowercase).collect()
}

fn resolved(path: &Path) -> PathBuf {
    path.ancestors()
        .find_map(|ancestor| fs::canonicalize(ancestor).ok().map(|real| (ancestor, real)))
        .and_then(|(ancestor, real)| path.strip_prefix(ancestor).ok().map(|rest| real.join(rest)))
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::{
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    const ROOT: &str = r"C:\Users\me\Downloads\recovery";

    fn guarded(path: &str) -> Result<PathBuf> {
        checked_path(Path::new(path), &[PathBuf::from(ROOT)])
    }

    fn invalid_reason(path: &str) -> String {
        let error = guarded(path).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{path}: {}", error.message);
        error.message
    }

    fn denied(path: &str) -> bool {
        guarded(path).is_err_and(|error| error.code == ErrorCode::Denied)
    }

    #[test]
    fn plain_paths_are_cleaned() {
        assert_eq!(guarded(r"c:/Users/me/Documents/a.txt").unwrap(), PathBuf::from(r"C:\Users\me\Documents\a.txt"));
        assert_eq!(guarded(r"D:\data\").unwrap(), PathBuf::from(r"D:\data"));
        assert_eq!(guarded(r"E:\").unwrap(), PathBuf::from(r"E:\"));
        assert_eq!(guarded(r"C:\Users\me\100% done.txt").unwrap(), PathBuf::from(r"C:\Users\me\100% done.txt"));
        assert_eq!(guarded(r"C:\Users\me\.config\a.b").unwrap(), PathBuf::from(r"C:\Users\me\.config\a.b"));
    }

    #[test]
    fn protected_folders_are_denied_on_component_boundaries() {
        assert!(denied(ROOT));
        assert!(denied(r"C:\Users\me\Downloads\recovery\"));
        assert!(denied(r"c:/users/me/downloads/RECOVERY/ssh/id_ed25519"));
        assert!(guarded(r"C:\Users\me\Downloads\recovery-old\a.txt").is_ok());
        assert!(guarded(r"C:\Users\me\Downloads\recover\a.txt").is_ok());
        assert!(guarded(r"C:\Users\me\Documents\a.txt").is_ok());
    }

    #[test]
    fn every_reviewed_bypass_is_rejected() {
        let cases = [
            (r"C:\Users\me\Downloads\x\..\recovery\a.txt", "'.' or '..' component"),
            (r"C:\Users\me\Downloads\.\recovery\a.txt", "'.' or '..' component"),
            (r"C:\Users\me\Downloads\\recovery\a.txt", "doubled separator"),
            (r"C:\Users\me\Downloads\recovery.\a.txt", "ending in '.' or a space"),
            (r"C:\Users\me\Downloads\recovery \a.txt", "ending in '.' or a space"),
            (r"C:\Users\me\DOWNLO~1\recovery\a.txt", "contains '~'"),
            (r"\\localhost\C$\Users\me\Downloads\recovery\a.txt", "UNC or device path"),
            (r"\\?\UNC\localhost\C$\Users\me\Downloads\recovery\a.txt", "UNC or device path"),
            (r"\\.\C:\Users\me\Downloads\recovery\a.txt", "UNC or device path"),
            (r"\\?\C:\Users\me\Downloads\recovery\a.txt", "UNC or device path"),
            (r"\\?\GLOBALROOT\Device\HarddiskVolume3\Users\me\Downloads\recovery\a.txt", "UNC or device path"),
            (r"//localhost/C$/Users/me/Downloads/recovery/a.txt", "UNC or device path"),
            (r"Downloads\recovery\a.txt", "relative path"),
            (r"\Users\me\Downloads\recovery\a.txt", "relative to the current drive"),
            (r"C:Users\me\Downloads\recovery\a.txt", "relative to the current folder"),
            (r"C:\Users\me\Downloads\recovery::$DATA", "contains ':' after the drive letter"),
            (r"%USERPROFILE%\Downloads\recovery\a.txt", "environment variables"),
            (r"C:\Users\%USERNAME%\Downloads\recovery\a.txt", "environment variables"),
            (r"~\Downloads\recovery\a.txt", "contains '~'"),
            (r"C:\Users\me\Downloads\recovery\a?.txt", "contains '?'"),
            ("C:\\Users\\me\\Downloads\\rec\u{0}overy", "contains '\\0'"),
            ("", "empty path"),
        ];
        for (path, reason) in cases {
            let message = invalid_reason(path);
            assert!(message.contains(reason), "{path}: {message}");
        }
    }

    struct TempTree(PathBuf);

    impl TempTree {
        fn new() -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
            let temp = temp.to_str().unwrap().strip_prefix(VERBATIM_PREFIX).unwrap().to_string();
            let base = PathBuf::from(temp).join(format!("deckhand-core-guard-{}-{nanos}", std::process::id()));
            fs::create_dir_all(base.join("Protected").join("Inner")).unwrap();
            fs::create_dir_all(base.join("Open")).unwrap();
            Self(base)
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }

        fn text(&self, relative: &str) -> String {
            self.path(relative).to_str().unwrap().to_string()
        }

        fn junction(&self, link: &str, target: &str) -> PathBuf {
            let link = self.path(link);
            let status = Command::new("cmd")
                .args(["/d", "/c", "mklink", "/j"])
                .arg(&link)
                .arg(self.path(target))
                .output()
                .unwrap()
                .status;
            assert!(status.success(), "mklink /j failed");
            link
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir(self.path("Link"));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn real_folders_are_compared_by_canonical_form() {
        let tree = TempTree::new();
        let protect = |root: PathBuf| move |path: &str| checked_path(Path::new(path), std::slice::from_ref(&root));
        let guard = protect(tree.path("Protected"));
        assert_eq!(guard(&tree.text(r"Protected\Inner\new.txt")).unwrap_err().code, ErrorCode::Denied);
        assert_eq!(guard(&tree.text(r"PROTECTED\inner\NEW.TXT")).unwrap_err().code, ErrorCode::Denied);
        assert_eq!(guard(&tree.text(r"Open\a.txt")).unwrap(), tree.path(r"Open\a.txt"));
        assert!(guard(&tree.text(r"Protected-old\a.txt")).is_ok());
        let verbatim_root = PathBuf::from(format!("{VERBATIM_PREFIX}{}", tree.text("protected").to_lowercase()));
        let guard = protect(verbatim_root);
        assert_eq!(guard(&tree.text(r"Protected\Inner")).unwrap_err().code, ErrorCode::Denied);
        assert!(guard(&tree.text(r"Open\b.txt")).is_ok());
    }

    #[test]
    fn junctions_into_a_protected_folder_are_denied() {
        let tree = TempTree::new();
        let link = tree.junction("Link", "Protected");
        let roots = [tree.path("Protected")];
        let through_link = link.join("Inner").join("secret.txt");
        assert_eq!(checked_path(&through_link, &roots).unwrap_err().code, ErrorCode::Denied);
        assert_eq!(checked_path(&link, &roots).unwrap_err().code, ErrorCode::Denied);
        let root_through_link = [link.clone()];
        let direct = tree.path(r"Protected\Inner\secret.txt");
        assert_eq!(checked_path(&direct, &root_through_link).unwrap_err().code, ErrorCode::Denied);
        assert!(checked_path(&tree.path(r"Open\a.txt"), &roots).is_ok());
    }

    #[test]
    fn empty_and_relative_roots_protect_nothing_by_accident() {
        let roots = [PathBuf::new()];
        assert!(checked_path(Path::new(r"C:\Users\me\a.txt"), &roots).is_ok());
        let relative = [PathBuf::from("deckhand-no-such-folder")];
        assert!(checked_path(Path::new(r"C:\Users\me\a.txt"), &relative).is_ok());
    }
}
