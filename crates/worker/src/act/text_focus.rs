use winpilot_core::{Element, Error, Role, text_limit::ellipsized};

const LABEL_NAME_CHARS: usize = 60;
const WAY_OUT: &str = "click the field first or give type a target";
const NOT_TEXT: [Role; 23] = [
    Role::Button,
    Role::SplitButton,
    Role::Checkbox,
    Role::Radio,
    Role::Link,
    Role::ListItem,
    Role::TreeItem,
    Role::Tab,
    Role::TabItem,
    Role::Menu,
    Role::MenuBar,
    Role::MenuItem,
    Role::Slider,
    Role::ScrollBar,
    Role::Thumb,
    Role::ProgressBar,
    Role::Header,
    Role::HeaderItem,
    Role::Separator,
    Role::ToolBar,
    Role::StatusBar,
    Role::TitleBar,
    Role::ToolTip,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextFocus {
    Accepts,
    Unknown,
    NotText,
    ReadOnly,
}

fn text_focus(element: &Element) -> TextFocus {
    if NOT_TEXT.contains(&element.role) {
        return TextFocus::NotText;
    }
    if element.patterns.value && element.state.read_only {
        return TextFocus::ReadOnly;
    }
    if element.role == Role::Edit || element.patterns.value {
        return TextFocus::Accepts;
    }
    TextFocus::Unknown
}

pub(crate) fn has_printable(text: &str) -> bool {
    text.chars().any(|ch| !ch.is_control())
}

pub(crate) fn refusal(focused: &Element) -> Option<Error> {
    let what = match text_focus(focused) {
        TextFocus::Accepts | TextFocus::Unknown => return None,
        TextFocus::NotText => label(focused),
        TextFocus::ReadOnly => format!("read-only {}", label(focused)),
    };
    Some(Error::unsupported(format!("the focused element is {what}, not a text field; {WAY_OUT}")))
}

fn label(element: &Element) -> String {
    let name = element.name.trim();
    if name.is_empty() {
        return element.role.to_string();
    }
    format!("{} \"{}\"", element.role, ellipsized(name, LABEL_NAME_CHARS))
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;
    use crate::test_fixture::element;

    fn focused(role: Role, name: &str) -> Element {
        let mut element = element(7, 2, role, name);
        element.state.focused = true;
        element.state.focusable = true;
        element
    }

    fn with_value(mut element: Element, read_only: bool) -> Element {
        element.patterns.value = true;
        element.state.read_only = read_only;
        element
    }

    #[test]
    fn controls_that_never_take_text_are_known() {
        for role in NOT_TEXT {
            assert_eq!(text_focus(&focused(role, "X")), TextFocus::NotText, "{role}");
        }
        assert_eq!(text_focus(&with_value(focused(Role::Link, "Docs"), true)), TextFocus::NotText);
    }

    #[test]
    fn fields_take_text() {
        assert_eq!(text_focus(&focused(Role::Edit, "Search")), TextFocus::Accepts);
        assert_eq!(text_focus(&with_value(focused(Role::Edit, "Search"), false)), TextFocus::Accepts);
        assert_eq!(text_focus(&with_value(focused(Role::Combobox, "Font"), false)), TextFocus::Accepts);
        assert_eq!(text_focus(&with_value(focused(Role::Document, "Body"), false)), TextFocus::Accepts);
        let mut password = focused(Role::Edit, "Password");
        password.state.password = true;
        assert_eq!(text_focus(&password), TextFocus::Accepts);
    }

    #[test]
    fn read_only_values_are_known_not_to_take_text() {
        assert_eq!(text_focus(&with_value(focused(Role::Edit, "Total"), true)), TextFocus::ReadOnly);
        assert_eq!(text_focus(&with_value(focused(Role::Document, "Page"), true)), TextFocus::ReadOnly);
        assert_eq!(text_focus(&with_value(focused(Role::Combobox, "Mode"), true)), TextFocus::ReadOnly);
        assert_eq!(text_focus(&with_value(focused(Role::Pane, ""), true)), TextFocus::ReadOnly);
    }

    #[test]
    fn generic_and_custom_surfaces_are_given_the_benefit_of_the_doubt() {
        for role in [
            Role::Pane,
            Role::Group,
            Role::Custom,
            Role::Window,
            Role::Unknown,
            Role::Document,
            Role::Text,
            Role::Image,
            Role::List,
            Role::Tree,
            Role::DataGrid,
            Role::DataItem,
            Role::Table,
            Role::Spinner,
        ] {
            assert_eq!(text_focus(&focused(role, "")), TextFocus::Unknown, "{role}");
            assert!(refusal(&focused(role, "")).is_none(), "{role}");
        }
    }

    #[test]
    fn refusals_name_the_focused_element_and_the_way_out() {
        let error = refusal(&focused(Role::Button, "Split")).unwrap();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(
            error.message,
            "the focused element is button \"Split\", not a text field; click the field first or give type a target"
        );
        let unnamed = refusal(&focused(Role::Slider, "  ")).unwrap();
        assert!(unnamed.message.starts_with("the focused element is slider, not a text field"), "{}", unnamed.message);
        let page = refusal(&with_value(focused(Role::Document, "Clipchamp"), true)).unwrap();
        assert!(
            page.message.starts_with("the focused element is read-only document \"Clipchamp\""),
            "{}",
            page.message
        );
        assert!(refusal(&focused(Role::Edit, "Name")).is_none());
    }

    #[test]
    fn long_names_are_clipped_in_the_refusal() {
        let error = refusal(&focused(Role::Button, &"x".repeat(200))).unwrap();
        assert!(error.message.contains("…\""), "{}", error.message);
        assert!(error.message.len() < 200);
    }

    #[test]
    fn only_printable_text_is_guarded() {
        assert!(has_printable("s"));
        assert!(has_printable(" "));
        assert!(has_printable("line\n"));
        assert!(!has_printable("\n"));
        assert!(!has_printable("\n\t\r"));
        assert!(!has_printable(""));
    }
}
