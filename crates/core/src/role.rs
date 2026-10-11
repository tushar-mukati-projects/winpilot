use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Button,
    Calendar,
    Checkbox,
    Combobox,
    Edit,
    Link,
    Image,
    ListItem,
    List,
    Menu,
    MenuBar,
    MenuItem,
    ProgressBar,
    Radio,
    ScrollBar,
    Slider,
    Spinner,
    StatusBar,
    Tab,
    TabItem,
    Text,
    ToolBar,
    ToolTip,
    Tree,
    TreeItem,
    Custom,
    Group,
    Thumb,
    DataGrid,
    DataItem,
    Document,
    SplitButton,
    Window,
    Pane,
    Header,
    HeaderItem,
    Table,
    TitleBar,
    Separator,
    SemanticZoom,
    AppBar,
    Unknown,
}

const CONTROL_TYPES: [Role; 41] = [
    Role::Button,
    Role::Calendar,
    Role::Checkbox,
    Role::Combobox,
    Role::Edit,
    Role::Link,
    Role::Image,
    Role::ListItem,
    Role::List,
    Role::Menu,
    Role::MenuBar,
    Role::MenuItem,
    Role::ProgressBar,
    Role::Radio,
    Role::ScrollBar,
    Role::Slider,
    Role::Spinner,
    Role::StatusBar,
    Role::Tab,
    Role::TabItem,
    Role::Text,
    Role::ToolBar,
    Role::ToolTip,
    Role::Tree,
    Role::TreeItem,
    Role::Custom,
    Role::Group,
    Role::Thumb,
    Role::DataGrid,
    Role::DataItem,
    Role::Document,
    Role::SplitButton,
    Role::Window,
    Role::Pane,
    Role::Header,
    Role::HeaderItem,
    Role::Table,
    Role::TitleBar,
    Role::Separator,
    Role::SemanticZoom,
    Role::AppBar,
];

const FIRST_CONTROL_TYPE_ID: i32 = 50000;

impl Role {
    pub fn from_control_type(id: i32) -> Self {
        usize::try_from(id - FIRST_CONTROL_TYPE_ID)
            .ok()
            .and_then(|index| CONTROL_TYPES.get(index).copied())
            .unwrap_or(Self::Unknown)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Calendar => "calendar",
            Self::Checkbox => "checkbox",
            Self::Combobox => "combobox",
            Self::Edit => "edit",
            Self::Link => "link",
            Self::Image => "image",
            Self::ListItem => "listitem",
            Self::List => "list",
            Self::Menu => "menu",
            Self::MenuBar => "menubar",
            Self::MenuItem => "menuitem",
            Self::ProgressBar => "progressbar",
            Self::Radio => "radio",
            Self::ScrollBar => "scrollbar",
            Self::Slider => "slider",
            Self::Spinner => "spinner",
            Self::StatusBar => "statusbar",
            Self::Tab => "tab",
            Self::TabItem => "tabitem",
            Self::Text => "text",
            Self::ToolBar => "toolbar",
            Self::ToolTip => "tooltip",
            Self::Tree => "tree",
            Self::TreeItem => "treeitem",
            Self::Custom => "custom",
            Self::Group => "group",
            Self::Thumb => "thumb",
            Self::DataGrid => "datagrid",
            Self::DataItem => "dataitem",
            Self::Document => "document",
            Self::SplitButton => "splitbutton",
            Self::Window => "window",
            Self::Pane => "pane",
            Self::Header => "header",
            Self::HeaderItem => "headeritem",
            Self::Table => "table",
            Self::TitleBar => "titlebar",
            Self::Separator => "separator",
            Self::SemanticZoom => "semanticzoom",
            Self::AppBar => "appbar",
            Self::Unknown => "unknown",
        }
    }

    pub const fn is_interactive(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Checkbox
                | Self::Combobox
                | Self::Edit
                | Self::Link
                | Self::ListItem
                | Self::MenuItem
                | Self::Radio
                | Self::Slider
                | Self::Spinner
                | Self::TabItem
                | Self::TreeItem
                | Self::DataItem
                | Self::SplitButton
                | Self::HeaderItem
        )
    }

    pub const fn is_container(self) -> bool {
        matches!(
            self,
            Self::List
                | Self::Menu
                | Self::MenuBar
                | Self::Tab
                | Self::ToolBar
                | Self::Tree
                | Self::Group
                | Self::DataGrid
                | Self::Document
                | Self::Window
                | Self::Pane
                | Self::Header
                | Self::Table
                | Self::StatusBar
                | Self::AppBar
        )
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let wanted = text.to_ascii_lowercase().replace(['_', '-', ' '], "");
        let alias = match wanted.as_str() {
            "hyperlink" | "a" => Some(Self::Link),
            "radiobutton" => Some(Self::Radio),
            "check" => Some(Self::Checkbox),
            "textbox" | "input" | "field" => Some(Self::Edit),
            "combo" | "dropdown" | "select" => Some(Self::Combobox),
            "row" => Some(Self::DataItem),
            "label" | "statictext" => Some(Self::Text),
            "tabs" => Some(Self::Tab),
            "dialog" => Some(Self::Window),
            _ => None,
        };
        alias
            .or_else(|| CONTROL_TYPES.iter().copied().find(|role| role.as_str() == wanted))
            .ok_or_else(|| Error::invalid(format!("unknown role '{text}'")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_uia_control_type_ids() {
        assert_eq!(Role::from_control_type(50000), Role::Button);
        assert_eq!(Role::from_control_type(50004), Role::Edit);
        assert_eq!(Role::from_control_type(50040), Role::AppBar);
        assert_eq!(Role::from_control_type(49999), Role::Unknown);
        assert_eq!(Role::from_control_type(50041), Role::Unknown);
    }

    #[test]
    fn parses_names_and_aliases() {
        assert_eq!("button".parse::<Role>().unwrap(), Role::Button);
        assert_eq!("MenuItem".parse::<Role>().unwrap(), Role::MenuItem);
        assert_eq!("hyperlink".parse::<Role>().unwrap(), Role::Link);
        assert_eq!("text-box".parse::<Role>().unwrap(), Role::Edit);
        assert!("gizmo".parse::<Role>().is_err());
    }

    #[test]
    fn every_role_round_trips_through_its_name() {
        for role in CONTROL_TYPES {
            assert_eq!(role.as_str().parse::<Role>().unwrap(), role);
        }
    }
}
