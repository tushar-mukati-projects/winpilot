use deckhand_core::{Result, protocol::WindowsReport};

use crate::{app_windows::app_windows, engine::Engine};

pub fn list(engine: &Engine) -> Result<WindowsReport> {
    Ok(WindowsReport { windows: app_windows(engine)?, ..WindowsReport::default() })
}

#[cfg(test)]
mod tests {
    use deckhand_core::format;

    use super::*;
    use crate::test_fixture::engine;

    #[test]
    fn the_list_shows_allowed_app_windows_one_per_line() {
        let report = list(engine()).unwrap();
        assert!(!report.windows.is_empty());
        assert!(report.windows.iter().all(|window| !engine().guard.denies(window)));
        assert!(report.note.is_none());
        assert_eq!(format::windows(&report).lines().count(), report.windows.len());
    }
}
