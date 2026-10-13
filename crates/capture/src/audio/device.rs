use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::Audio::{
    AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_E_RESOURCES_INVALIDATED,
    AUDCLNT_E_SERVICE_NOT_RUNNING, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
    eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows_core::HRESULT;
use winpilot_core::{Error, ErrorCode, Result};

const E_NOTFOUND: HRESULT = HRESULT(0x8007_0490_u32 as i32);
const MICROPHONE_DENIED: &str = "Windows is blocking microphone access for desktop apps; the user must turn on \
Settings > Privacy & security > Microphone > \"Let desktop apps access your microphone\" (WinPilot never changes it)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endpoint {
    Loopback,
    Microphone,
}

impl Endpoint {
    const fn device_name(self) -> &'static str {
        match self {
            Self::Loopback => "playback device",
            Self::Microphone => "microphone",
        }
    }

    const fn missing(self) -> &'static str {
        match self {
            Self::Loopback => "no playback device is available, so system audio can't be recorded",
            Self::Microphone => "no microphone is available",
        }
    }
}

pub(crate) fn activate_default_client(endpoint: Endpoint) -> Result<IAudioClient> {
    let device = default_device(endpoint)?;
    unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }
        .map_err(|error| audio_error(endpoint, &error, "opening"))
}

fn default_device(endpoint: Endpoint) -> Result<IMMDevice> {
    let enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|error| Error::unsupported(format!("Windows audio is not available: {error}")))?;
    let flow = match endpoint {
        Endpoint::Loopback => eRender,
        Endpoint::Microphone => eCapture,
    };
    unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }
        .map_err(|error| audio_error(endpoint, &error, "finding"))
}

pub(crate) fn audio_error(endpoint: Endpoint, error: &windows_core::Error, doing: &str) -> Error {
    classify(endpoint, error.code())
        .unwrap_or_else(|| Error::internal(format!("{doing} the {}: {error}", endpoint.device_name())))
}

fn classify(endpoint: Endpoint, code: HRESULT) -> Option<Error> {
    let name = endpoint.device_name();
    match code {
        E_NOTFOUND => Some(Error::not_found(endpoint.missing())),
        E_ACCESSDENIED if endpoint == Endpoint::Microphone => Some(Error::new(ErrorCode::NeedsUser, MICROPHONE_DENIED)),
        AUDCLNT_E_DEVICE_INVALIDATED | AUDCLNT_E_RESOURCES_INVALIDATED => {
            Some(Error::not_found(format!("the {name} was disconnected or changed")))
        }
        AUDCLNT_E_DEVICE_IN_USE => {
            Some(Error::new(ErrorCode::Busy, format!("another app is using the {name} exclusively")))
        }
        AUDCLNT_E_SERVICE_NOT_RUNNING => Some(Error::unsupported("the Windows Audio service is not running")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::E_FAIL;

    use super::*;

    #[test]
    fn known_failures_get_clear_messages() {
        let missing = classify(Endpoint::Loopback, E_NOTFOUND).unwrap();
        assert_eq!(missing.code, ErrorCode::NotFound);
        assert!(missing.message.contains("system audio"), "{}", missing.message);
        let lost = classify(Endpoint::Microphone, AUDCLNT_E_DEVICE_INVALIDATED).unwrap();
        assert_eq!(lost.message, "the microphone was disconnected or changed");
        assert_eq!(classify(Endpoint::Microphone, AUDCLNT_E_DEVICE_IN_USE).unwrap().code, ErrorCode::Busy);
        assert_eq!(classify(Endpoint::Loopback, E_FAIL), None);
    }

    #[test]
    fn a_denied_microphone_asks_the_user_without_changing_settings() {
        let denied = classify(Endpoint::Microphone, E_ACCESSDENIED).unwrap();
        assert_eq!(denied.code, ErrorCode::NeedsUser);
        assert!(denied.message.contains("Let desktop apps access your microphone"), "{}", denied.message);
        assert!(denied.message.contains("never changes"), "{}", denied.message);
        assert_eq!(classify(Endpoint::Loopback, E_ACCESSDENIED), None);
    }

    #[test]
    fn other_failures_name_the_device_and_the_step() {
        let error = audio_error(Endpoint::Loopback, &windows_core::Error::from(E_FAIL), "starting");
        assert_eq!(error.code, ErrorCode::Internal);
        assert!(error.message.starts_with("starting the playback device: "), "{}", error.message);
    }
}
