use windows::Win32::Foundation::{E_ACCESSDENIED, E_INVALIDARG, E_OUTOFMEMORY};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
    DXGI_ERROR_SESSION_DISCONNECTED, DXGI_ERROR_WAIT_TIMEOUT,
};
use windows::core::HRESULT;

const TRANSIENT_CODES: [HRESULT; 6] = [
    E_ACCESSDENIED,
    E_INVALIDARG,
    E_OUTOFMEMORY,
    DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
    DXGI_ERROR_SESSION_DISCONNECTED,
    DXGI_ERROR_WAIT_TIMEOUT,
];
const LOST_CODES: [HRESULT; 3] = [DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DxgiFailure {
    AccessLost,
    Transient,
    Unsupported,
}

pub(super) fn classify_creation(error: &windows::core::Error) -> DxgiFailure {
    match classify_runtime(error) {
        DxgiFailure::Transient if !TRANSIENT_CODES.contains(&error.code()) => DxgiFailure::Unsupported,
        failure => failure,
    }
}

pub(super) fn classify_runtime(error: &windows::core::Error) -> DxgiFailure {
    if LOST_CODES.contains(&error.code()) { DxgiFailure::AccessLost } else { DxgiFailure::Transient }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::{E_FAIL, E_NOINTERFACE};
    use windows::Win32::Graphics::Dxgi::DXGI_ERROR_UNSUPPORTED;

    use super::*;

    fn creation(code: HRESULT) -> DxgiFailure {
        classify_creation(&windows::core::Error::from_hresult(code))
    }

    fn runtime(code: HRESULT) -> DxgiFailure {
        classify_runtime(&windows::core::Error::from_hresult(code))
    }

    #[test]
    fn creation_failures_are_permanent_unless_known_transient() {
        assert!(matches!(creation(DXGI_ERROR_UNSUPPORTED), DxgiFailure::Unsupported));
        assert!(matches!(creation(E_NOINTERFACE), DxgiFailure::Unsupported));
        assert!(matches!(creation(E_FAIL), DxgiFailure::Unsupported));
        assert!(matches!(creation(E_ACCESSDENIED), DxgiFailure::Transient));
        assert!(matches!(creation(E_INVALIDARG), DxgiFailure::Transient));
        assert!(matches!(creation(DXGI_ERROR_NOT_CURRENTLY_AVAILABLE), DxgiFailure::Transient));
        assert!(matches!(creation(DXGI_ERROR_ACCESS_LOST), DxgiFailure::AccessLost));
    }

    #[test]
    fn runtime_failures_reopen_or_retry_later() {
        assert!(matches!(runtime(DXGI_ERROR_ACCESS_LOST), DxgiFailure::AccessLost));
        assert!(matches!(runtime(DXGI_ERROR_DEVICE_REMOVED), DxgiFailure::AccessLost));
        assert!(matches!(runtime(E_FAIL), DxgiFailure::Transient));
    }
}
