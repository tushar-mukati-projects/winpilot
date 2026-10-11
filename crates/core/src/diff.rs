use serde::{Deserialize, Serialize};

use crate::element::{Element, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Added { element: Element },
    Removed { element: Element },
    Changed { before: Element, after: Element },
}

pub fn diff(before: &Snapshot, after: &Snapshot) -> Vec<Change> {
    todo!("{before:?} {after:?}")
}
