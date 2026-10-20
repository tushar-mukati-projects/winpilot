use crate::{element::Element, matching::fold};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Rank {
    text: u32,
    visible: bool,
    enabled: bool,
    actionable: bool,
}

pub(super) fn rank(element: &Element, texts: &[&str]) -> Rank {
    Rank {
        text: texts.iter().map(|text| text_rank(&element.name, text)).sum(),
        visible: element.is_visible(),
        enabled: element.state.enabled,
        actionable: element.is_actionable(),
    }
}

fn text_rank(name: &str, text: &str) -> u32 {
    let name = fold(name);
    let text = fold(text);
    if name == text { 2 } else { u32::from(name.starts_with(&text)) }
}
