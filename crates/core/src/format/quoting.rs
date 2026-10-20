use crate::{
    matching::is_gap,
    object_marks::without_object_marks,
    text_limit::{ELLIPSIS, clipped},
};

pub(super) fn quote(text: &str, limit: usize) -> String {
    let flat = collapse(&without_object_marks(text), is_gap);
    let (kept, cut) = clipped(&flat, limit);
    let mut quoted = String::with_capacity(kept.len() + 8);
    quoted.push('"');
    for ch in kept.chars() {
        if matches!(ch, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    if cut {
        quoted.push(ELLIPSIS);
    }
    quoted
}

pub(super) fn single_line(text: &str, limit: usize) -> String {
    let flat = collapse(&without_object_marks(text), char::is_control);
    let (kept, cut) = clipped(&flat, limit);
    let mut line = kept.to_string();
    if cut {
        line.push(ELLIPSIS);
    }
    line
}

fn collapse(text: &str, is_break: fn(char) -> bool) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut after_gap = false;
    for ch in text.chars() {
        let gap = is_break(ch);
        match (gap, after_gap) {
            (false, _) => collapsed.push(ch),
            (true, false) => collapsed.push(' '),
            (true, true) => {}
        }
        after_gap = gap;
    }
    collapsed
}
