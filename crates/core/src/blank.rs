pub(crate) fn non_blank(value: Option<&String>) -> Option<String> {
    value.map(|text| text.trim()).filter(|text| !text.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_text_counts_as_missing_and_the_rest_is_trimmed() {
        assert_eq!(non_blank(None), None);
        assert_eq!(non_blank(Some(&"  ".to_string())), None);
        assert_eq!(non_blank(Some(&" Save As ".to_string())).as_deref(), Some("Save As"));
    }
}
