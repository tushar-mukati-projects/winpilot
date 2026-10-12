use std::path::{Path, PathBuf};

use deckhand_core::{Error, ErrorCode, Result};

pub(crate) fn check_launch_args(args: &str, check: impl Fn(&Path) -> Result<PathBuf>) -> Result<()> {
    let words = split_args(args);
    for (index, word) in words.iter().enumerate() {
        let Some(path) = path_in(word) else { continue };
        check(Path::new(path)).map_err(|error| in_args(&error))?;
        check_run_on(path, &words[index + 1..], &check)?;
    }
    Ok(())
}

fn split_args(args: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for ch in args.chars() {
        match ch {
            '"' => quoted = !quoted,
            ch if ch.is_whitespace() && !quoted => push_word(&mut words, &mut word),
            ch => word.push(ch),
        }
    }
    push_word(&mut words, &mut word);
    words
}

fn path_in(word: &str) -> Option<&str> {
    let bytes = word.as_bytes();
    (0..bytes.len()).find(|&index| starts_a_path(bytes, index)).map(|index| &word[index..])
}

fn check_run_on(first: &str, rest: &[String], check: &impl Fn(&Path) -> Result<PathBuf>) -> Result<()> {
    let mut joined = first.to_string();
    for word in rest {
        joined.push(' ');
        joined.push_str(word);
        match check(Path::new(&joined)) {
            Err(error) if error.code == ErrorCode::Denied => return Err(in_args(&error)),
            Err(_) => return Ok(()),
            Ok(_) => {}
        }
    }
    Ok(())
}

fn starts_a_path(bytes: &[u8], index: usize) -> bool {
    let before = index.checked_sub(1).map(|previous| bytes[previous]);
    let rest = &bytes[index..];
    let drive = matches!(rest, [letter, b':', b'\\' | b'/', ..] if letter.is_ascii_alphabetic());
    let unc = matches!(rest, [b'\\' | b'/', b'\\' | b'/', ..]);
    match before {
        None | Some(b'=') => drive || unc,
        Some(previous) => drive && !previous.is_ascii_alphanumeric(),
    }
}

fn push_word(words: &mut Vec<String>, word: &mut String) {
    if !word.is_empty() {
        words.push(std::mem::take(word));
    }
}

fn in_args(error: &Error) -> Error {
    Error::new(error.code, format!("args: {}", error.message))
}

#[cfg(test)]
mod tests {
    use deckhand_core::config::GuardConfig;

    use super::*;

    fn guard() -> GuardConfig {
        let deny_paths = vec![PathBuf::from("C:\\Vault"), PathBuf::from("C:\\Users\\me\\My Vault")];
        GuardConfig { deny_paths, ..GuardConfig::default() }
    }

    fn checked(args: &str) -> Result<()> {
        check_launch_args(args, |path| guard().checked_path(path))
    }

    fn code(args: &str) -> Option<ErrorCode> {
        checked(args).err().map(|error| error.code)
    }

    #[test]
    fn words_split_on_spaces_outside_quotes() {
        assert_eq!(split_args(r#"--title "a b"  C:\x.txt"#), ["--title", "a b", "C:\\x.txt"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn paths_are_found_alone_after_options_and_inside_urls() {
        assert_eq!(path_in("C:\\Vault\\a.txt"), Some("C:\\Vault\\a.txt"));
        assert_eq!(path_in("--out=D:/data/x"), Some("D:/data/x"));
        assert_eq!(path_in("/out:C:\\x"), Some("C:\\x"));
        assert_eq!(path_in("file:///C:/Vault/a.txt"), Some("C:/Vault/a.txt"));
        assert_eq!(path_in("\\\\server\\share"), Some("\\\\server\\share"));
        assert_eq!(path_in("https://example.com/a"), None);
        assert_eq!(path_in("--verbose"), None);
    }

    #[test]
    fn denied_folders_are_refused_however_the_path_is_written() {
        for args in [
            "C:\\Vault\\keys.txt",
            "\"C:\\Vault\\my keys.txt\"",
            "--out=C:\\Vault\\x.txt",
            "-n C:/vault/x.txt",
            "C:\\Users\\me\\My Vault\\notes.txt",
            "file:///C:/Vault/a.txt",
        ] {
            assert_eq!(code(args), Some(ErrorCode::Denied), "{args}");
        }
        assert!(checked("C:\\Vault\\keys.txt").unwrap_err().message.starts_with("args: "));
    }

    #[test]
    fn paths_that_cannot_be_checked_are_refused() {
        assert_eq!(code("\\\\localhost\\C$\\Vault\\a.txt"), Some(ErrorCode::InvalidArgument));
        assert_eq!(code("C:\\Open\\..\\Vault\\a.txt"), Some(ErrorCode::InvalidArgument));
    }

    #[test]
    fn ordinary_arguments_pass() {
        for args in ["", "--new-window https://example.com", "C:\\Open\\a.txt --line 3", "C:\\a.txt C:\\b.txt"] {
            assert!(checked(args).is_ok(), "{args}");
        }
    }
}
