#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Open,
    Save,
}

impl DialogKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Save => "Save",
        }
    }
}
