pub(crate) fn toast_xml(title: &str, message: &str) -> String {
    let lines: Vec<String> = [title, message]
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .map(|text| format!("<text>{}</text>", escaped(text)))
        .collect();
    format!("<toast><visual><binding template=\"ToastGeneric\">{}</binding></visual></toast>", lines.concat())
}

fn escaped(text: &str) -> String {
    text.chars().map(escaped_char).collect()
}

fn escaped_char(character: char) -> String {
    match character {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        '\'' => "&apos;".to_string(),
        '\t' | '\n' | '\r' => character.to_string(),
        control if control.is_control() => " ".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_message_become_two_generic_text_lines() {
        assert_eq!(
            toast_xml("Export", "Export finished"),
            "<toast><visual><binding template=\"ToastGeneric\"><text>Export</text><text>Export finished</text></binding></visual></toast>"
        );
    }

    #[test]
    fn an_empty_title_is_left_out() {
        assert_eq!(
            toast_xml(" ", "Done"),
            "<toast><visual><binding template=\"ToastGeneric\"><text>Done</text></binding></visual></toast>"
        );
    }

    #[test]
    fn markup_characters_are_escaped_and_controls_blanked() {
        assert_eq!(escaped("a<b>&\"c'd"), "a&lt;b&gt;&amp;&quot;c&apos;d");
        assert_eq!(escaped("line\nnext\u{7}bell"), "line\nnext bell");
        assert_eq!(escaped("Café ☕"), "Café ☕");
    }
}
