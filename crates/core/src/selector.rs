use std::str::FromStr;

use crate::{
    element::{Element, Snapshot, WindowInfo},
    error::{Error, Result},
    role::Role,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Role(Role),
    Name(String),
    NameContains(String),
    Id(String),
    Class(String),
    Text(String),
    Window(String),
    Process(String),
    Nth(u32),
    Focused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorStep {
    pub terms: Vec<Term>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub steps: Vec<SelectorStep>,
}

impl FromStr for Selector {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        todo!("parse selector '{text}'")
    }
}

impl Selector {
    pub fn has_window_terms(&self) -> bool {
        todo!()
    }

    pub fn has_element_terms(&self) -> bool {
        todo!()
    }

    pub fn matches_window(&self, window: &WindowInfo) -> bool {
        todo!("{window:?}")
    }

    pub fn find<'a>(&self, snapshot: &'a Snapshot) -> Vec<&'a Element> {
        todo!("{snapshot:?}")
    }

    pub fn resolve<'a>(&self, snapshot: &'a Snapshot) -> Result<&'a Element> {
        todo!("{snapshot:?}")
    }
}
