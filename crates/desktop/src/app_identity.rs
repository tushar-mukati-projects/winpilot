use windows::Win32::{
    Foundation::ERROR_SUCCESS,
    System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW},
};
use windows::core::{PCWSTR, w};

pub(crate) const APP_USER_MODEL_ID: &str = "Deckhand.Agent";
const REGISTRATION_KEY: PCWSTR = w!("Software\\Classes\\AppUserModelId\\Deckhand.Agent");
const DISPLAY_NAME: PCWSTR = w!("DisplayName");

pub(crate) fn is_registered() -> bool {
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, REGISTRATION_KEY, DISPLAY_NAME, RRF_RT_REG_SZ, None, None, Some(&raw mut size))
    };
    status == ERROR_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registration_key_names_the_app_id() {
        let key = unsafe { REGISTRATION_KEY.to_string() }.unwrap();
        assert!(key.ends_with(&format!("\\{APP_USER_MODEL_ID}")), "{key}");
    }

    #[test]
    fn the_registration_can_be_checked() {
        let _registered = is_registered();
    }
}
