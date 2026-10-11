use serde::{Deserialize, Serialize};

use crate::{
    geometry::Rect,
    refs::{ElementRef, WindowRef},
    role::Role,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ElementState {
    pub enabled: bool,
    pub focused: bool,
    pub focusable: bool,
    pub offscreen: bool,
    pub read_only: bool,
    pub password: bool,
    pub toggled: Option<bool>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Patterns {
    pub invoke: bool,
    pub value: bool,
    pub toggle: bool,
    pub selection_item: bool,
    pub expand_collapse: bool,
    pub scroll: bool,
    pub scroll_item: bool,
    pub text: bool,
    pub range_value: bool,
    pub legacy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementRef,
    pub depth: u16,
    pub role: Role,
    pub name: String,
    pub value: Option<String>,
    pub automation_id: String,
    pub class_name: String,
    pub rect: Rect,
    pub state: ElementState,
    pub patterns: Patterns,
}

impl Element {
    pub fn is_actionable(&self) -> bool {
        self.role.is_interactive()
            || self.patterns.invoke
            || self.patterns.toggle
            || self.patterns.selection_item
            || self.patterns.expand_collapse
            || (self.patterns.value && !self.state.read_only)
    }

    pub fn is_visible(&self) -> bool {
        !self.state.offscreen && !self.rect.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub hwnd: WindowRef,
    pub title: String,
    pub process: String,
    pub pid: u32,
    pub class_name: String,
    pub rect: Rect,
    pub state: WindowState,
    pub foreground: bool,
    pub elevated: bool,
    #[serde(default)]
    pub desktop: Option<u32>,
    #[serde(default = "on_current_desktop")]
    pub on_current_desktop: bool,
}

const fn on_current_desktop() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub window: WindowInfo,
    pub elements: Vec<Element>,
    pub focused: Option<ElementRef>,
    pub truncated: bool,
}

impl Snapshot {
    pub fn get(&self, id: ElementRef) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    pub fn index_of(&self, id: ElementRef) -> Option<usize> {
        self.elements.iter().position(|element| element.id == id)
    }

    pub fn ancestors(&self, index: usize) -> Vec<&Element> {
        let mut chain = Vec::new();
        let Some(mut depth) = self.elements.get(index).map(|element| element.depth) else { return chain };
        for element in self.elements[..index].iter().rev() {
            if element.depth < depth {
                depth = element.depth;
                chain.push(element);
            }
        }
        chain.reverse();
        chain
    }

    pub fn descendants(&self, index: usize) -> &[Element] {
        let Some(depth) = self.elements.get(index).map(|element| element.depth) else { return &[] };
        let rest = &self.elements[index + 1..];
        let end = rest.iter().position(|element| element.depth <= depth).unwrap_or(rest.len());
        &rest[..end]
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::geometry::Rect;

    pub fn window(title: &str) -> WindowInfo {
        WindowInfo {
            hwnd: WindowRef(0x10),
            title: title.to_string(),
            process: "notepad.exe".to_string(),
            pid: 100,
            class_name: "Notepad".to_string(),
            rect: Rect::new(0, 0, 800, 600),
            state: WindowState::Normal,
            foreground: true,
            elevated: false,
            desktop: None,
            on_current_desktop: true,
        }
    }

    pub fn element(id: u32, depth: u16, role: Role, name: &str) -> Element {
        Element {
            id: ElementRef(id),
            depth,
            role,
            name: name.to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::new(10, 10, 50, 20),
            state: ElementState { enabled: true, ..ElementState::default() },
            patterns: Patterns::default(),
        }
    }

    pub fn snapshot(elements: Vec<Element>) -> Snapshot {
        Snapshot { window: window("Untitled - Notepad"), elements, focused: None, truncated: false }
    }

    pub fn app_window(hwnd: i64, process: &str, title: &str) -> WindowInfo {
        WindowInfo { hwnd: WindowRef(hwnd), process: process.to_string(), foreground: false, ..window(title) }
    }

    pub fn with_value(mut element: Element, value: &str) -> Element {
        element.value = Some(value.to_string());
        element
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{self, element, snapshot};
    use super::*;

    fn sample() -> Snapshot {
        snapshot(vec![
            element(1, 0, Role::Window, "Notepad"),
            element(2, 1, Role::MenuBar, "Application"),
            element(3, 2, Role::MenuItem, "File"),
            element(4, 2, Role::MenuItem, "Edit"),
            element(5, 1, Role::Edit, "Text editor"),
        ])
    }

    #[test]
    fn ancestors_walk_up_by_depth() {
        let snap = sample();
        let names: Vec<&str> = snap.ancestors(3).iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Notepad", "Application"]);
    }

    #[test]
    fn windows_from_older_workers_count_as_on_the_current_desktop() {
        let mut json = serde_json::to_value(fixtures::window("Notes")).unwrap();
        let fields = json.as_object_mut().unwrap();
        fields.remove("desktop");
        fields.remove("on_current_desktop");
        let window: WindowInfo = serde_json::from_value(json).unwrap();
        assert!(window.on_current_desktop);
        assert_eq!(window.desktop, None);
    }

    #[test]
    fn descendants_stop_at_sibling() {
        let snap = sample();
        let names: Vec<&str> = snap.descendants(1).iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["File", "Edit"]);
        assert!(snap.descendants(4).is_empty());
    }
}
