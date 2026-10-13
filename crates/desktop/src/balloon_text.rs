use winpilot_core::{Error, Result};

const TITLE_UNITS: usize = 64;
const MESSAGE_UNITS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BalloonText {
    pub(crate) title: [u16; TITLE_UNITS],
    pub(crate) message: [u16; MESSAGE_UNITS],
}

impl BalloonText {
    pub(crate) fn new(title: &str, message: &str) -> Result<Self> {
        if message.trim().is_empty() {
            return Err(Error::invalid("the notification message must not be empty"));
        }
        Ok(Self { title: wide_buffer("title", title)?, message: wide_buffer("message", message)? })
    }
}

pub(crate) fn wide_buffer<const N: usize>(field: &str, text: &str) -> Result<[u16; N]> {
    if text.contains('\0') {
        return Err(Error::invalid(format!("the notification {field} must not contain NUL characters")));
    }
    let length = text.encode_utf16().count();
    if length >= N {
        return Err(Error::invalid(format!(
            "the notification {field} holds at most {} UTF-16 units, got {length}",
            N.saturating_sub(1)
        )));
    }
    let mut buffer = [0u16; N];
    for (slot, unit) in buffer.iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Shell::NOTIFYICONDATAW;
    use winpilot_core::{
        ErrorCode,
        protocol::{NOTIFY_MESSAGE_MAX, NOTIFY_TITLE_MAX},
    };

    use super::*;

    fn decoded(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&unit| unit == 0).unwrap();
        String::from_utf16(&buffer[..end]).unwrap()
    }

    fn refusal<const N: usize>(text: &str) -> String {
        let error = wide_buffer::<N>("title", text).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        error.message
    }

    #[test]
    fn buffers_match_the_shell_structure_and_the_core_limits() {
        let data = NOTIFYICONDATAW::default();
        assert_eq!(data.szInfoTitle.len(), TITLE_UNITS);
        assert_eq!(data.szInfo.len(), MESSAGE_UNITS);
        assert_eq!(NOTIFY_TITLE_MAX + 1, TITLE_UNITS);
        assert_eq!(NOTIFY_MESSAGE_MAX + 1, MESSAGE_UNITS);
    }

    #[test]
    fn text_round_trips_and_ends_with_nul() {
        let text = BalloonText::new("Café ☕ 日本", "Export finished\nsecond line").unwrap();
        assert_eq!(decoded(&text.title), "Café ☕ 日本");
        assert_eq!(decoded(&text.message), "Export finished\nsecond line");
        assert!(text.title[9..].iter().all(|&unit| unit == 0));
    }

    #[test]
    fn the_last_unit_is_kept_for_the_terminating_nul() {
        let title = wide_buffer::<TITLE_UNITS>("title", &"t".repeat(63)).unwrap();
        assert_eq!((title[62], title[63]), (u16::from(b't'), 0));
        assert_eq!(
            refusal::<TITLE_UNITS>(&"t".repeat(64)),
            "the notification title holds at most 63 UTF-16 units, got 64"
        );
        let message = wide_buffer::<MESSAGE_UNITS>("message", &"m".repeat(255)).unwrap();
        assert_eq!(message[255], 0);
        assert!(wide_buffer::<MESSAGE_UNITS>("message", &"m".repeat(256)).is_err());
    }

    #[test]
    fn surrogate_pairs_count_as_two_units() {
        assert!(wide_buffer::<TITLE_UNITS>("title", &"🎉".repeat(31)).is_ok());
        assert!(refusal::<TITLE_UNITS>(&"🎉".repeat(32)).ends_with("got 64"));
        let one = wide_buffer::<4>("title", "🎉").unwrap();
        assert_eq!(decoded(&one), "🎉");
    }

    #[test]
    fn nul_and_empty_text_are_refused_instead_of_cut() {
        assert_eq!(refusal::<TITLE_UNITS>("Do\0ne"), "the notification title must not contain NUL characters");
        let empty = BalloonText::new("Title", " \n").unwrap_err();
        assert_eq!(empty.code, ErrorCode::InvalidArgument);
        assert!(BalloonText::new("", "untitled is fine for the shell").is_ok());
    }
}
