use winpilot_core::Result;
use winpilot_desktop::clipboard;

use super::{
    done::Done,
    mode::{Mode, check_before},
};
use crate::{call_scope::CallScope, engine::Engine};

pub(crate) fn step(engine: &Engine, text: &str, mode: Mode, call: &CallScope) -> Result<Done> {
    check_before(engine, None, mode, call)?;
    clipboard::set_text(text)?;
    Ok(Done::noting(set_note(text)))
}

fn set_note(text: &str) -> String {
    match text.chars().count() {
        1 => "clipboard set (1 char)".to_string(),
        count => format!("clipboard set ({count} chars)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_counts_characters_not_bytes() {
        assert_eq!(set_note("héllo"), "clipboard set (5 chars)");
        assert_eq!(set_note("x"), "clipboard set (1 char)");
        assert_eq!(set_note(""), "clipboard set (0 chars)");
    }
}
