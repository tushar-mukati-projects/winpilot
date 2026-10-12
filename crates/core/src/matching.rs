pub(crate) fn fold(text: &str) -> String {
    let words: Vec<&str> = text.split(is_gap).filter(|word| !word.is_empty()).collect();
    words.join(" ").to_lowercase()
}

pub(crate) fn contains_folded(haystack: &str, needle: &str) -> bool {
    fold(haystack).contains(&fold(needle))
}

pub(crate) fn process_stem(process: &str) -> String {
    let folded = fold(process);
    match folded.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => folded,
    }
}

pub(crate) fn same_process(left: &str, right: &str) -> bool {
    process_stem(left) == process_stem(right)
}

pub(crate) fn is_gap(ch: char) -> bool {
    ch.is_whitespace() || ch.is_control()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_lowercases_and_collapses_whitespace_and_controls() {
        assert_eq!(fold("  Save   As  "), "save as");
        assert_eq!(fold("line one\nline two"), "line one line two");
        assert_eq!(fold("a\t\r\n b\u{7}c"), "a b c");
        assert_eq!(fold(""), "");
        assert_eq!(fold(" \n\t "), "");
    }

    #[test]
    fn contains_folded_ignores_case_and_spacing() {
        assert!(contains_folded("Untitled -   Notepad", "untitled - notepad"));
        assert!(!contains_folded("Notepad", "chrome"));
    }

    #[test]
    fn process_stems_drop_an_optional_exe() {
        assert_eq!(process_stem("KeePass.EXE"), "keepass");
        assert_eq!(process_stem("keepass"), "keepass");
        assert!(same_process("notepad", "NOTEPAD.exe"));
        assert!(!same_process("note", "notepad.exe"));
    }
}
