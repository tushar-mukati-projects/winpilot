use deckhand_core::{Error, ErrorCode, Result};
use windows::Win32::{
    Foundation::{CO_E_OBJNOTCONNECTED, E_ACCESSDENIED, E_NOINTERFACE, RPC_E_DISCONNECTED},
    UI::Accessibility::{
        UIA_E_ELEMENTNOTAVAILABLE, UIA_E_ELEMENTNOTENABLED, UIA_E_INVALIDOPERATION, UIA_E_NOTSUPPORTED, UIA_E_TIMEOUT,
    },
};
use windows_core::HRESULT;

const RPC_SERVER_UNAVAILABLE: u32 = 0x8007_06BA;
const DISCONNECTED: u32 = RPC_E_DISCONNECTED.0 as u32;
const NOT_CONNECTED: u32 = CO_E_OBJNOTCONNECTED.0 as u32;
const ACCESS_DENIED: u32 = E_ACCESSDENIED.0 as u32;
const NO_INTERFACE: u32 = E_NOINTERFACE.0 as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    Gone,
    NotResponding,
    AccessDenied,
    Disabled,
    NotSupported,
    Other,
}

pub(crate) fn classify(code: HRESULT) -> Failure {
    match code.0 as u32 {
        UIA_E_ELEMENTNOTAVAILABLE | DISCONNECTED | NOT_CONNECTED | RPC_SERVER_UNAVAILABLE => Failure::Gone,
        UIA_E_TIMEOUT => Failure::NotResponding,
        ACCESS_DENIED => Failure::AccessDenied,
        UIA_E_ELEMENTNOTENABLED => Failure::Disabled,
        UIA_E_NOTSUPPORTED | UIA_E_INVALIDOPERATION => Failure::NotSupported,
        _ => Failure::Other,
    }
}

pub(crate) fn is_gone(error: &windows_core::Error) -> bool {
    classify(error.code()) == Failure::Gone
}

pub(crate) fn is_null_result(error: &windows_core::Error) -> bool {
    error.code().is_ok()
}

fn is_pattern_absent(error: &windows_core::Error) -> bool {
    let code = error.code().0 as u32;
    is_null_result(error) || code == NO_INTERFACE || code == UIA_E_NOTSUPPORTED
}

pub(crate) fn optional<T>(result: windows_core::Result<T>) -> windows_core::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if is_null_result(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn optional_pattern<T>(result: windows_core::Result<T>) -> windows_core::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if is_pattern_absent(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn uia_error(what: &str, error: &windows_core::Error) -> Error {
    match classify(error.code()) {
        Failure::Gone => {
            Error::new(ErrorCode::StaleRef, format!("{what}: the element no longer exists; observe again"))
        }
        Failure::NotResponding => {
            Error::new(ErrorCode::TargetNotResponding, format!("{what}: the app did not answer UI Automation in time"))
        }
        Failure::AccessDenied => Error::new(
            ErrorCode::ElevatedTarget,
            format!("{what}: access denied; the app is probably running as administrator"),
        ),
        Failure::Disabled => Error::unsupported(format!("{what}: the element is disabled")),
        Failure::NotSupported => Error::unsupported(format!("{what}: the element does not allow this right now")),
        Failure::Other => Error::internal(format!("{what}: {}", describe(error))),
    }
}

fn describe(error: &windows_core::Error) -> String {
    let code = error.code().0 as u32;
    let message = error.message();
    if message.is_empty() { format!("HRESULT 0x{code:08X}") } else { format!("{message} (0x{code:08X})") }
}

pub(crate) trait UiaResult<T> {
    fn uia(self, what: &str) -> Result<T>;
}

impl<T> UiaResult<T> for windows_core::Result<T> {
    fn uia(self, what: &str) -> Result<T> {
        self.map_err(|error| uia_error(what, &error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure_from(code: u32) -> windows_core::Error {
        windows_core::Error::from_hresult(HRESULT(code as i32))
    }

    #[test]
    fn classifies_uia_codes() {
        assert_eq!(classify(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)), Failure::Gone);
        assert_eq!(classify(HRESULT(UIA_E_TIMEOUT as i32)), Failure::NotResponding);
        assert_eq!(classify(E_ACCESSDENIED), Failure::AccessDenied);
        assert_eq!(classify(RPC_E_DISCONNECTED), Failure::Gone);
        assert_eq!(classify(HRESULT(0x8000_4005_u32 as i32)), Failure::Other);
    }

    #[test]
    fn maps_to_error_codes() {
        assert_eq!(uia_error("x", &failure_from(UIA_E_ELEMENTNOTAVAILABLE)).code, ErrorCode::StaleRef);
        assert_eq!(uia_error("x", &failure_from(UIA_E_TIMEOUT)).code, ErrorCode::TargetNotResponding);
        assert_eq!(uia_error("x", &failure_from(ACCESS_DENIED)).code, ErrorCode::ElevatedTarget);
        assert_eq!(uia_error("x", &failure_from(UIA_E_ELEMENTNOTENABLED)).code, ErrorCode::Unsupported);
        let other = uia_error("reading e3", &failure_from(0x8000_4005));
        assert_eq!(other.code, ErrorCode::Internal);
        assert!(other.message.starts_with("reading e3: "));
        assert!(other.message.contains("0x80004005"));
    }

    #[test]
    fn null_results_become_none() {
        let empty: windows_core::Result<u8> = Err(windows_core::Error::empty());
        assert_eq!(optional(empty).unwrap(), None);
        assert_eq!(optional(Ok(4)).unwrap(), Some(4));
        assert!(optional::<u8>(Err(failure_from(UIA_E_TIMEOUT))).is_err());
    }

    #[test]
    fn missing_patterns_become_none() {
        assert_eq!(optional_pattern::<u8>(Err(failure_from(NO_INTERFACE))).unwrap(), None);
        assert_eq!(optional_pattern::<u8>(Err(failure_from(UIA_E_NOTSUPPORTED))).unwrap(), None);
        assert!(optional_pattern::<u8>(Err(failure_from(UIA_E_ELEMENTNOTAVAILABLE))).is_err());
    }
}
