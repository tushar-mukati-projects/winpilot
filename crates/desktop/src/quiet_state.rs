use windows::Win32::UI::Shell::{
    QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME,
    QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuietReason {
    QuietTime,
    FullScreen,
    Presentation,
    Away,
}

impl QuietReason {
    pub const fn describe(self) -> &'static str {
        match self {
            Self::QuietTime => "the first hour after sign-in or a Windows upgrade",
            Self::FullScreen => "a full-screen app or game",
            Self::Presentation => "presentation mode",
            Self::Away => "the lock screen or screen saver",
        }
    }
}

pub(crate) fn current_quiet_reason() -> Option<QuietReason> {
    let state = unsafe { SHQueryUserNotificationState() }.ok()?;
    quiet_reason(state)
}

fn quiet_reason(state: QUERY_USER_NOTIFICATION_STATE) -> Option<QuietReason> {
    match state {
        QUNS_QUIET_TIME => Some(QuietReason::QuietTime),
        QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN => Some(QuietReason::FullScreen),
        QUNS_PRESENTATION_MODE => Some(QuietReason::Presentation),
        QUNS_NOT_PRESENT => Some(QuietReason::Away),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Shell::{QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP};

    use super::*;

    #[test]
    fn accepting_states_and_running_store_apps_are_not_quiet() {
        assert_eq!(quiet_reason(QUNS_ACCEPTS_NOTIFICATIONS), None);
        assert_eq!(quiet_reason(QUNS_APP), None);
        assert_eq!(quiet_reason(QUERY_USER_NOTIFICATION_STATE(99)), None);
    }

    #[test]
    fn quiet_states_map_to_their_reasons() {
        assert_eq!(quiet_reason(QUNS_QUIET_TIME), Some(QuietReason::QuietTime));
        assert_eq!(quiet_reason(QUNS_BUSY), Some(QuietReason::FullScreen));
        assert_eq!(quiet_reason(QUNS_RUNNING_D3D_FULL_SCREEN), Some(QuietReason::FullScreen));
        assert_eq!(quiet_reason(QUNS_PRESENTATION_MODE), Some(QuietReason::Presentation));
        assert_eq!(quiet_reason(QUNS_NOT_PRESENT), Some(QuietReason::Away));
    }

    #[test]
    fn reasons_read_after_during() {
        assert_eq!(QuietReason::QuietTime.describe(), "the first hour after sign-in or a Windows upgrade");
        assert_eq!(QuietReason::Away.describe(), "the lock screen or screen saver");
    }

    #[test]
    fn the_live_state_can_be_queried() {
        let _reason = current_quiet_reason();
    }
}
