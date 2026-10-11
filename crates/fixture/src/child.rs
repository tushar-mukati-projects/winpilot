use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDlgItem, SW_HIDE, SW_SHOWNA, SendMessageW, SetWindowTextW, ShowWindow,
};
use windows::core::{HSTRING, Result};

pub fn item(window: HWND, id: i32) -> Result<HWND> {
    unsafe { GetDlgItem(Some(window), id) }
}

pub fn set_text(window: HWND, id: i32, text: &str) -> Result<()> {
    let control = item(window, id)?;
    unsafe { SetWindowTextW(control, &HSTRING::from(text)) }
}

pub fn send(control: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { SendMessageW(control, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }.0
}

pub fn index_from(result: isize) -> Option<usize> {
    usize::try_from(result).ok()
}

pub fn reveal(control: HWND) {
    let _was_visible = unsafe { ShowWindow(control, SW_SHOWNA) };
}

pub fn conceal(control: HWND) {
    let _was_visible = unsafe { ShowWindow(control, SW_HIDE) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_results_have_no_index() {
        assert_eq!(index_from(-1), None);
        assert_eq!(index_from(0), Some(0));
        assert_eq!(index_from(29), Some(29));
    }
}
