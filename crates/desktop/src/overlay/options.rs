const NEUTRAL_LABEL: &str = "AI agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayOptions {
    pub animations: bool,
    pub capturable: bool,
    pub kill_hotkey: Option<String>,
    pub label: String,
    pub palette: Vec<[u8; 3]>,
}

impl Default for OverlayOptions {
    fn default() -> Self {
        Self {
            animations: true,
            capturable: false,
            kill_hotkey: None,
            label: NEUTRAL_LABEL.to_string(),
            palette: Vec::new(),
        }
    }
}
