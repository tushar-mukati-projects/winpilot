use std::mem::size_of;

use deckhand_core::{Context, Error, Result};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Shell::{
            NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIIF_RESPECT_QUIET_TIME, NIM_ADD,
            NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFY_ICON_DATA_FLAGS, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4,
            NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{HICON, IDI_INFORMATION, LoadIconW},
    },
};

use crate::balloon_text::{BalloonText, wide_buffer};

const ICON_ID: u32 = 1;
const TOOLTIP: &str = "Deckhand";

#[derive(Debug)]
pub(crate) struct TrayIcon {
    hwnd: HWND,
}

impl TrayIcon {
    pub(crate) fn add(hwnd: HWND, callback_message: u32) -> Result<Self> {
        let icon = Self { hwnd };
        let mut data = icon.data(NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP);
        data.uCallbackMessage = callback_message;
        data.hIcon = information_icon()?;
        data.szTip = wide_buffer("tooltip", TOOLTIP)?;
        tell_shell(NIM_ADD, &data, "adding the Deckhand notification icon")?;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        tell_shell(NIM_SETVERSION, &data, "setting the notification icon version")?;
        Ok(icon)
    }

    pub(crate) fn show_balloon(&self, text: &BalloonText) -> Result<()> {
        let mut data = self.data(NIF_INFO);
        data.szInfoTitle = text.title;
        data.szInfo = text.message;
        data.dwInfoFlags = NIIF_INFO | NIIF_RESPECT_QUIET_TIME;
        tell_shell(NIM_MODIFY, &data, "showing the notification")
    }

    fn data(&self, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
        icon_data(self.hwnd, flags)
    }
}

fn icon_data(hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: flags,
        ..NOTIFYICONDATAW::default()
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let data = self.data(NOTIFY_ICON_DATA_FLAGS(0));
        let _removed = unsafe { Shell_NotifyIconW(NIM_DELETE, &raw const data) };
    }
}

fn information_icon() -> Result<HICON> {
    unsafe { LoadIconW(None, IDI_INFORMATION) }.context("loading the information icon")
}

fn tell_shell(message: NOTIFY_ICON_MESSAGE, data: &NOTIFYICONDATAW, doing: &str) -> Result<()> {
    if unsafe { Shell_NotifyIconW(message, data) }.as_bool() {
        return Ok(());
    }
    let reason = windows::core::Error::from_thread();
    Err(Error::unsupported(format!(
        "{doing} failed; the notification area refused it (is Explorer running?): {reason}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_data_names_the_window_icon_and_fields() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let data = icon_data(hwnd, NIF_INFO);
        assert_eq!(data.cbSize as usize, size_of::<NOTIFYICONDATAW>());
        assert_eq!((data.hWnd, data.uID, data.uFlags), (hwnd, ICON_ID, NIF_INFO));
        assert!(data.szInfo.iter().all(|&unit| unit == 0));
    }

    #[test]
    fn the_tooltip_fits() {
        let tip: [u16; 128] = wide_buffer("tooltip", TOOLTIP).unwrap();
        let units: Vec<u16> = TOOLTIP.encode_utf16().collect();
        assert_eq!(&tip[..units.len()], units.as_slice());
        assert_eq!(tip[units.len()], 0);
        assert_eq!(NOTIFYICONDATAW::default().szTip.len(), tip.len());
    }
}
