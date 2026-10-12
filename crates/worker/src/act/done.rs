#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Done {
    pub(crate) method: Option<String>,
    pub(crate) note: Option<String>,
}

impl Done {
    pub(crate) fn by(method: impl Into<String>) -> Self {
        Self { method: Some(method.into()), note: None }
    }

    pub(crate) fn noting(note: impl Into<String>) -> Self {
        Self { method: None, note: Some(note.into()) }
    }

    #[must_use]
    pub(crate) fn with_note(mut self, note: Option<String>) -> Self {
        let Some(note) = note else { return self };
        self.note = Some(joined(self.note.take(), note));
        self
    }
}

pub(crate) fn joined(earlier: Option<String>, note: String) -> String {
    match earlier {
        Some(earlier) => format!("{earlier}; {note}"),
        None => note,
    }
}

pub(crate) fn via_focus_click(method: &str, clicked: bool) -> String {
    if clicked { format!("click-focus+{method}") } else { method.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_accumulate_in_order() {
        let done = Done::by("keys").with_note(Some("first".to_string())).with_note(None);
        assert_eq!(done.note.as_deref(), Some("first"));
        let done = done.with_note(Some("second".to_string()));
        assert_eq!(done.note.as_deref(), Some("first; second"));
        assert_eq!(done.method.as_deref(), Some("keys"));
    }

    #[test]
    fn a_note_alone_has_no_method() {
        assert_eq!(Done::noting("already on"), Done { method: None, note: Some("already on".to_string()) });
    }

    #[test]
    fn focus_clicks_are_named_in_the_method() {
        assert_eq!(via_focus_click("keys", false), "keys");
        assert_eq!(via_focus_click("paste", true), "click-focus+paste");
    }
}
