use deckhand_core::WindowState;
use windows::Win32::UI::WindowsAndMessaging::{
    SHOW_WINDOW_CMD, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    Activate,
    KeepForeground,
}

impl Activation {
    pub(crate) const fn launch_command(self) -> SHOW_WINDOW_CMD {
        match self {
            Self::Activate => SW_SHOWNORMAL,
            Self::KeepForeground => SW_SHOWNOACTIVATE,
        }
    }

    pub(crate) const fn restore_command(self) -> SHOW_WINDOW_CMD {
        match self {
            Self::Activate => SW_RESTORE,
            Self::KeepForeground => SW_SHOWNOACTIVATE,
        }
    }

    pub(crate) const fn state_command(self, state: WindowState) -> SHOW_WINDOW_CMD {
        match (state, self) {
            (WindowState::Minimized, Self::Activate) => SW_MINIMIZE,
            (WindowState::Minimized, Self::KeepForeground) => SW_SHOWMINNOACTIVE,
            (WindowState::Maximized, _) => SW_MAXIMIZE,
            (WindowState::Normal, _) => self.restore_command(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeping_the_foreground_picks_the_no_activate_commands() {
        assert_eq!(Activation::KeepForeground.launch_command(), SW_SHOWNOACTIVATE);
        assert_eq!(Activation::KeepForeground.restore_command(), SW_SHOWNOACTIVATE);
        assert_eq!(Activation::KeepForeground.state_command(WindowState::Minimized), SW_SHOWMINNOACTIVE);
        assert_eq!(Activation::KeepForeground.state_command(WindowState::Normal), SW_SHOWNOACTIVATE);
    }

    #[test]
    fn activating_keeps_the_usual_commands() {
        assert_eq!(Activation::Activate.launch_command(), SW_SHOWNORMAL);
        assert_eq!(Activation::Activate.state_command(WindowState::Minimized), SW_MINIMIZE);
        assert_eq!(Activation::Activate.state_command(WindowState::Normal), SW_RESTORE);
    }

    #[test]
    fn maximizing_has_no_no_activate_command() {
        assert_eq!(Activation::Activate.state_command(WindowState::Maximized), SW_MAXIMIZE);
        assert_eq!(Activation::KeepForeground.state_command(WindowState::Maximized), SW_MAXIMIZE);
    }
}
