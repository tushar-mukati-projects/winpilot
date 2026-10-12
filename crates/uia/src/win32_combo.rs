use deckhand_core::{Error, Result};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::{
        Accessibility::IUIAutomationElement,
        Controls::{COMBOBOXINFO, GetComboBoxInfo},
        WindowsAndMessaging::{
            CB_GETCURSEL, CB_SETCURSEL, GetDlgCtrlID, LBN_SELCHANGE, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_COMMAND,
        },
    },
};

const ANSWER_WAIT_MS: u32 = 2000;

pub(crate) struct Win32Combo {
    combo: HWND,
    list: HWND,
}

impl Win32Combo {
    pub(crate) fn of(element: &IUIAutomationElement) -> Option<Self> {
        let window = unsafe { element.CurrentNativeWindowHandle() }.ok().filter(|window| !window.is_invalid())?;
        let info = combo_box_info(window)?;
        Some(Self { combo: info.hwndCombo, list: info.hwndList })
    }

    pub(crate) fn selected_index(&self) -> Option<usize> {
        let index = send(self.combo, CB_GETCURSEL, WPARAM(0), LPARAM(0))?;
        usize::try_from(index).ok()
    }

    pub(crate) fn keep_selection(&self, index: usize) -> Result<()> {
        if self.selected_index() != Some(index) && send(self.combo, CB_SETCURSEL, WPARAM(index), LPARAM(0)).is_none() {
            return Err(Error::unsupported("the combobox did not answer when asked to keep the choice"));
        }
        if self.selected_index() != Some(index) {
            return Err(Error::unsupported("the combobox dropped the choice when its list closed"));
        }
        self.announce_selection()
    }

    fn announce_selection(&self) -> Result<()> {
        let list_id = unsafe { GetDlgCtrlID(self.list) };
        let notification = WPARAM(selection_change(list_id));
        send(self.combo, WM_COMMAND, notification, LPARAM(self.list.0 as isize))
            .map(|_| ())
            .ok_or_else(|| Error::unsupported("the combobox did not answer when told about the choice"))
    }
}

pub(crate) fn dropdown_list(combo: HWND) -> Option<HWND> {
    combo_box_info(combo).map(|info| info.hwndList)
}

fn combo_box_info(window: HWND) -> Option<COMBOBOXINFO> {
    let mut info = COMBOBOXINFO { cbSize: size_of::<COMBOBOXINFO>() as u32, ..COMBOBOXINFO::default() };
    unsafe { GetComboBoxInfo(window, &raw mut info) }.ok()?;
    (!info.hwndCombo.is_invalid() && !info.hwndList.is_invalid()).then_some(info)
}

fn selection_change(list_id: i32) -> usize {
    ((LBN_SELCHANGE as usize) << 16) | (list_id as usize & 0xFFFF)
}

fn send(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<isize> {
    let mut result = 0usize;
    let answered = unsafe {
        SendMessageTimeoutW(window, message, wparam, lparam, SMTO_ABORTIFHUNG, ANSWER_WAIT_MS, Some(&raw mut result))
    };
    (answered.0 != 0).then_some(result as isize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notification_carries_the_list_id_and_the_change_code() {
        assert_eq!(selection_change(1000), 0x0001_03E8);
        assert_eq!(selection_change(-1), 0x0001_FFFF);
    }

    #[test]
    fn no_window_has_no_dropdown_list() {
        assert_eq!(dropdown_list(HWND::default()), None);
    }
}
