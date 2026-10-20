use std::time::Duration;

use winpilot_core::config::ActConfig;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
const TRANSACTION_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiaOptions {
    pub connection_timeout: Duration,
    pub transaction_timeout: Duration,
    pub first_ref: u32,
}

impl Default for UiaOptions {
    fn default() -> Self {
        Self { connection_timeout: CONNECTION_TIMEOUT, transaction_timeout: TRANSACTION_TIMEOUT, first_ref: 1 }
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
        let act = ActConfig::default();
        Self {
            max_elements: act.max_elements,
            include_offscreen: false,
            budget: Duration::from_millis(act.snapshot_budget_ms),
        }
    }
}
