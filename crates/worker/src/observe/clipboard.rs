use deckhand_core::{
    Error, ErrorCode, Result,
    protocol::{ClipboardReport, ClipboardRequest},
    text_limit::truncated,
};
use deckhand_desktop::clipboard::{self, ClipboardRead};

use crate::{engine::Engine, guard::Guard};

const MARKED_PRIVATE: &str =
    "the app that filled the clipboard marked it private (as password managers do), so Deckhand won't read it";

pub(crate) fn read(engine: &Engine, request: ClipboardRequest) -> Result<ClipboardReport> {
    let read = clipboard::read()?;
    check_allowed(&engine.guard, &read)?;
    let Some(text) = read.text else { return Ok(ClipboardReport { text: None, truncated: false }) };
    let (text, truncated) = truncated(text, request.max_chars);
    Ok(ClipboardReport { text: Some(text), truncated })
}

fn check_allowed(guard: &Guard, read: &ClipboardRead) -> Result<()> {
    if read.marked_private {
        return Err(Error::new(ErrorCode::Denied, MARKED_PRIVATE));
    }
    let Some(owner) = &read.owner else { return Ok(()) };
    if guard.denies_titled(&owner.process, &owner.title) {
        return Err(Error::new(
            ErrorCode::Denied,
            format!("the clipboard was filled by {}, which is on Deckhand's deny list", owner.process),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use deckhand_desktop::clipboard::ClipboardOwner;

    use super::*;
    use crate::test_fixture::engine;

    fn read_by(process: &str, marked_private: bool) -> ClipboardRead {
        let owner = ClipboardOwner { process: process.to_string(), title: String::new() };
        ClipboardRead { text: Some("secret".to_string()), marked_private, owner: Some(owner) }
    }

    #[test]
    fn a_copy_marked_private_is_refused() {
        let refused = check_allowed(&engine().guard, &read_by("notepad.exe", true)).unwrap_err();
        assert_eq!(refused.code, ErrorCode::Denied);
        assert_eq!(refused.message, MARKED_PRIVATE);
    }

    #[test]
    fn a_copy_from_a_denied_app_is_refused() {
        let refused = check_allowed(&engine().guard, &read_by("KeePass.exe", false)).unwrap_err();
        assert_eq!(refused.code, ErrorCode::Denied);
        assert!(refused.message.contains("KeePass.exe"), "{}", refused.message);
    }

    #[test]
    fn ordinary_copies_and_ownerless_clipboards_are_read() {
        assert!(check_allowed(&engine().guard, &read_by("notepad.exe", false)).is_ok());
        let ownerless = ClipboardRead { text: None, marked_private: false, owner: None };
        assert!(check_allowed(&engine().guard, &ownerless).is_ok());
    }

    #[test]
    fn reading_the_live_clipboard_never_fails_on_the_guard_alone() {
        let request = ClipboardRequest { max_chars: 5 };
        match read(engine(), request) {
            Ok(report) => assert!(report.text.as_ref().is_none_or(|text| text.chars().count() <= 5)),
            Err(error) => assert!(matches!(error.code, ErrorCode::Denied | ErrorCode::Busy), "{error}"),
        }
    }
}
