#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogKind {
    Open,
    Save,
}

impl DialogKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Save => "Save",
        }
    }
}
