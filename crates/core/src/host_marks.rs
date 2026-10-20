const PROTECTED_ID_PREFIX: &str = "winpilot-protected";
const OPT_IN_MARKER_ID: &str = "winpilot-protects-terminals";

pub fn is_protected_id(automation_id: &str) -> bool {
    automation_id.trim().starts_with(PROTECTED_ID_PREFIX)
}

pub fn is_opt_in_marker(automation_id: &str) -> bool {
    automation_id.trim() == OPT_IN_MARKER_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_ids_share_one_prefix() {
        assert!(is_protected_id("winpilot-protected-terminal-tm2"));
        assert!(is_protected_id(" winpilot-protected "));
        assert!(!is_protected_id("terminal-tm2"));
        assert!(!is_protected_id(""));
    }

    #[test]
    fn the_opt_in_marker_is_not_itself_protected() {
        assert!(is_opt_in_marker(OPT_IN_MARKER_ID));
        assert!(!is_protected_id(OPT_IN_MARKER_ID));
        assert!(!is_opt_in_marker("winpilot-protected-terminal-tm2"));
    }
}
