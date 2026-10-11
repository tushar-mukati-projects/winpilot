use std::sync::Arc;

use deckhand_core::WindowRef;

use crate::win;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowIdentity {
    pub(crate) hwnd: WindowRef,
    pub(crate) pid: u32,
    pub(crate) class: Arc<str>,
}

impl WindowIdentity {
    pub(crate) fn unknown() -> Self {
        Self { hwnd: WindowRef(0), pid: 0, class: Arc::from("") }
    }

    pub(crate) fn read(window: WindowRef) -> Option<Self> {
        if !win::window_exists(window) {
            return None;
        }
        Some(Self { hwnd: window, pid: win::process_id(window)?, class: Arc::from(win::class_name(window)?) })
    }

    pub(crate) fn is_known(&self) -> bool {
        self.hwnd.0 != 0
    }

    pub(crate) fn is_current(&self) -> bool {
        Self::read(self.hwnd).is_some_and(|current| current == *self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::taskbar;

    #[test]
    fn the_taskbar_reads_back_as_itself() {
        let identity = WindowIdentity::read(taskbar().hwnd).unwrap();
        assert_eq!(&*identity.class, "Shell_TrayWnd");
        assert!(identity.is_known());
        assert!(identity.is_current());
    }

    #[test]
    fn a_reused_handle_in_another_process_or_class_is_not_current() {
        let identity = WindowIdentity::read(taskbar().hwnd).unwrap();
        let other_process = WindowIdentity { pid: identity.pid.wrapping_add(1), ..identity.clone() };
        let other_class = WindowIdentity { class: Arc::from("Notepad"), ..identity };
        assert!(!other_process.is_current());
        assert!(!other_class.is_current());
    }

    #[test]
    fn missing_windows_have_no_identity() {
        assert_eq!(WindowIdentity::read(WindowRef(0)), None);
        assert!(!WindowIdentity::unknown().is_known());
        assert!(!WindowIdentity::unknown().is_current());
    }
}
