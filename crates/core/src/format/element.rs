use crate::{element::Element, object_marks::without_object_marks, ocr, role::Role};

use super::{
    NAME_LIMIT, VALUE_LIMIT,
    quoting::{quote, single_line},
};

const PASSWORD_MASK: &str = "•••";

pub fn element_line(element: &Element, coords: bool) -> String {
    let mut parts = vec![element.id.to_string(), element.role.to_string()];
    parts.extend(element_label(element));
    parts.extend(element_value(element));
    parts.extend(element_flags(element).map(str::to_string));
    if coords {
        parts.push(ocr::coords(element.rect));
    }
    parts.join(" ")
}

fn element_label(element: &Element) -> Option<String> {
    let name = without_object_marks(&element.name);
    let name = name.trim();
    if !name.is_empty() {
        return Some(quote(name, NAME_LIMIT));
    }
    let id = element.automation_id.trim();
    (!id.is_empty()).then(|| format!("#{}", single_line(id, NAME_LIMIT)))
}

fn element_value(element: &Element) -> Option<String> {
    let value = element.value.as_deref().filter(|value| !without_object_marks(value).is_empty())?;
    if element.state.password {
        return Some(format!("value={PASSWORD_MASK}"));
    }
    Some(format!("value={}", quote(value, VALUE_LIMIT)))
}

fn element_flags(element: &Element) -> impl Iterator<Item = &'static str> {
    let state = element.state;
    [
        state.focused.then_some("focused"),
        (!state.enabled).then_some("disabled"),
        state.toggled.map(|on| if on { "checked" } else { "unchecked" }),
        state.expanded.map(|open| if open { "expanded" } else { "collapsed" }),
        (state.selected == Some(true)).then_some("selected"),
        (state.read_only && shows_read_only(element.role)).then_some("readonly"),
        state.password.then_some("password"),
        state.offscreen.then_some("offscreen"),
    ]
    .into_iter()
    .flatten()
}

const fn shows_read_only(role: Role) -> bool {
    matches!(role, Role::Edit | Role::Combobox | Role::Document)
}
