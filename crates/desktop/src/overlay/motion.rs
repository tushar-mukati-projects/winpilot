use std::ptr;

use windows::{
    Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    },
    core::BOOL,
};

pub(crate) fn system_animations_enabled() -> bool {
    let mut enabled = BOOL::from(true);
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(ptr::from_mut(&mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_err() || enabled.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_the_animation_setting_does_not_fail() {
        let _enabled = system_animations_enabled();
    }
}
