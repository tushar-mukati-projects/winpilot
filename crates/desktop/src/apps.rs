use std::path::Path;

use deckhand_core::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    Packaged,
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    pub id: String,
    pub kind: AppKind,
}

pub fn installed_apps() -> Result<Vec<AppEntry>> {
    todo!()
}

pub fn launch(app: &AppEntry, args: Option<&str>) -> Result<Option<u32>> {
    todo!("{app:?} {args:?}")
}

pub fn launch_path(path: &Path, args: Option<&str>) -> Result<Option<u32>> {
    todo!("{path:?} {args:?}")
}
