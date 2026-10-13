use winpilot_core::{
    Result, Target,
    refs::{looks_like_element_ref, looks_like_window_ref},
    target::parse_text_target,
};

use crate::window_spec::looks_like_title;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadTarget {
    DefaultWindow,
    Window(String),
    Element(Target),
}

pub(crate) fn read_target(spec: Option<&str>) -> Result<ReadTarget> {
    let Some(text) = spec.map(str::trim).filter(|text| !text.is_empty()) else { return Ok(ReadTarget::DefaultWindow) };
    if looks_like_window_ref(text) || (!looks_like_element_ref(text) && looks_like_title(text)) {
        return Ok(ReadTarget::Window(text.to_string()));
    }
    match parse_text_target(text)? {
        Target::Selector(selector) if !selector.has_element_terms() => Ok(ReadTarget::Window(text.to_string())),
        target => Ok(ReadTarget::Element(target)),
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::{ElementRef, ErrorCode};

    use super::*;

    fn element_selector(text: &str) -> bool {
        matches!(read_target(Some(text)).unwrap(), ReadTarget::Element(Target::Selector(_)))
    }

    fn window(text: &str) -> ReadTarget {
        ReadTarget::Window(text.to_string())
    }

    #[test]
    fn nothing_means_the_default_window() {
        assert_eq!(read_target(None).unwrap(), ReadTarget::DefaultWindow);
        assert_eq!(read_target(Some("   ")).unwrap(), ReadTarget::DefaultWindow);
    }

    #[test]
    fn refs_pick_their_kind() {
        assert_eq!(read_target(Some("e12")).unwrap(), ReadTarget::Element(Target::Element(ElementRef(12))));
        assert_eq!(read_target(Some(" w42 ")).unwrap(), window("w42"));
    }

    #[test]
    fn plain_text_is_a_window_title() {
        assert_eq!(read_target(Some("Untitled - Notepad")).unwrap(), window("Untitled - Notepad"));
        assert_eq!(read_target(Some("edit")).unwrap(), window("edit"));
    }

    #[test]
    fn titles_with_colons_stay_titles() {
        assert_eq!(read_target(Some("C:\\a.txt - Notepad")).unwrap(), window("C:\\a.txt - Notepad"));
        assert_eq!(read_target(Some("Re: lunch")).unwrap(), window("Re: lunch"));
    }

    #[test]
    fn window_only_selectors_are_windows() {
        assert_eq!(read_target(Some("process:notepad")).unwrap(), window("process:notepad"));
        assert_eq!(read_target(Some("window:\"Notepad\"")).unwrap(), window("window:\"Notepad\""));
    }

    #[test]
    fn selectors_with_element_terms_are_elements() {
        assert!(element_selector("role:button name:\"Save\""));
        assert!(element_selector("window:\"Notepad\" >> role:edit"));
        assert!(element_selector("id:1148"));
    }

    #[test]
    fn malformed_selectors_are_invalid() {
        assert_eq!(read_target(Some("role:gizmo")).unwrap_err().code, ErrorCode::InvalidArgument);
    }
}
