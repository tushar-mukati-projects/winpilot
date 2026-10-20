use std::fmt;

use super::{FOCUSED, Selector, SelectorStep, Term};

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_joined(f, &self.steps, " >> ")
    }
}

impl fmt::Display for SelectorStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_joined(f, &self.terms, " ")
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Role(role) => write!(f, "role:{role}"),
            Self::Name(value) => write_keyed(f, "name", value),
            Self::NameContains(value) => write_keyed(f, "name~", value),
            Self::Id(value) => write_keyed(f, "id", value),
            Self::Class(value) => write_keyed(f, "class", value),
            Self::Text(value) => write_keyed(f, "text", value),
            Self::Ocr(value) => write_keyed(f, "ocr", value),
            Self::Window(value) => write_keyed(f, "window", value),
            Self::Process(value) => write_keyed(f, "process", value),
            Self::Nth(position) => write!(f, "nth:{position}"),
            Self::Focused => f.write_str(FOCUSED),
        }
    }
}

fn write_joined<T: fmt::Display>(f: &mut fmt::Formatter<'_>, items: &[T], separator: &str) -> fmt::Result {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            f.write_str(separator)?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

fn write_keyed(f: &mut fmt::Formatter<'_>, key: &str, value: &str) -> fmt::Result {
    if needs_quotes(value) {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        write!(f, "{key}:\"{escaped}\"")
    } else {
        write!(f, "{key}:{value}")
    }
}

fn needs_quotes(value: &str) -> bool {
    value.is_empty() || value.chars().any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\' | '>'))
}
