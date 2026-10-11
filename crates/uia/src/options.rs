use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiaOptions {
    pub connection_timeout: Duration,
    pub transaction_timeout: Duration,
}

impl Default for UiaOptions {
    fn default() -> Self {
        Self { connection_timeout: Duration::from_secs(2), transaction_timeout: Duration::from_secs(3) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOptions {
    pub max_elements: usize,
    pub include_offscreen: bool,
    pub budget: Duration,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self { max_elements: 3000, include_offscreen: false, budget: Duration::from_millis(2500) }
    }
}
