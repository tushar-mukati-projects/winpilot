use crate::{
    element::{Element, WindowInfo},
    matching::{contains_folded, fold, same_process},
    role::Role,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Term {
    Role(Role),
    Name(String),
    NameContains(String),
    Id(String),
    Class(String),
    Text(String),
    Ocr(String),
    Window(String),
    Process(String),
    Nth(u32),
    Focused,
}

impl Term {
    pub(super) const fn position(&self) -> Option<u32> {
        match self {
            Self::Nth(position) => Some(*position),
            _ => None,
        }
    }

    pub(super) const fn is_window_term(&self) -> bool {
        matches!(self, Self::Window(_) | Self::Process(_))
    }

    pub(super) const fn is_element_term(&self) -> bool {
        !matches!(self, Self::Window(_) | Self::Process(_) | Self::Nth(_))
    }

    pub(super) fn matches_window(&self, window: &WindowInfo) -> bool {
        match self {
            Self::Window(title) => contains_folded(&window.title, title),
            Self::Process(process) => same_process(&window.process, process),
            _ => true,
        }
    }

    pub(super) fn matches_element(&self, element: &Element) -> bool {
        match self {
            Self::Role(role) => element.role == *role,
            Self::Name(name) => fold(&element.name) == fold(name),
            Self::NameContains(name) => contains_folded(&element.name, name),
            Self::Id(id) => element.automation_id == *id,
            Self::Class(class) => fold(&element.class_name) == fold(class),
            Self::Text(text) => {
                contains_folded(&element.name, text)
                    || element.value.as_deref().is_some_and(|value| contains_folded(value, text))
            }
            Self::Focused => element.state.focused,
            Self::Ocr(_) => false,
            Self::Window(_) | Self::Process(_) | Self::Nth(_) => true,
        }
    }
}
