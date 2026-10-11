use std::{marker::PhantomData, thread, time::Duration};

use deckhand_core::{Context, Error, ErrorCode, Result};
use windows::Win32::{
    Foundation::{GetLastError, HANDLE, HGLOBAL, NO_ERROR},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
            IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
        },
        Ole::{CF_LOCALE, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT},
    },
};

use crate::{
    clipboard_history::{PrivacyFormats, history_exclusion_markers},
    global_memory::{LockedGlobal, OwnedGlobal},
    window_identity::{owning_pid, title},
    window_label::process_name,
};

const OPEN_ATTEMPTS: u32 = 20;
const OPEN_RETRY_GAP: Duration = Duration::from_millis(10);
const TEXT_FORMATS: [u32; 4] = [CF_UNICODETEXT.0 as u32, CF_TEXT.0 as u32, CF_OEMTEXT.0 as u32, CF_LOCALE.0 as u32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardOwner {
    pub process: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardRead {
    pub text: Option<String>,
    pub marked_private: bool,
    pub owner: Option<ClipboardOwner>,
}

pub(crate) struct ClipboardData {
    pub(crate) format: u32,
    pub(crate) memory: OwnedGlobal,
}

struct OpenClipboardGuard {
    _bound_to_thread: PhantomData<*const ()>,
}

impl OpenClipboardGuard {
    fn open() -> Result<Self> {
        for attempt in 1..=OPEN_ATTEMPTS {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self { _bound_to_thread: PhantomData });
            }
            if attempt < OPEN_ATTEMPTS {
                thread::sleep(OPEN_RETRY_GAP);
            }
        }
        Err(Error::new(ErrorCode::Busy, "another app is holding the clipboard open"))
    }
}

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn get_text() -> Result<Option<String>> {
    let _clipboard = OpenClipboardGuard::open()?;
    unicode_text()
}

pub fn read() -> Result<ClipboardRead> {
    let _clipboard = OpenClipboardGuard::open()?;
    let marked_private = PrivacyFormats::registered()?.marks_private(&clipboard_formats()?, dword_value);
    let owner = owner();
    let text = if marked_private { None } else { unicode_text()? };
    Ok(ClipboardRead { text, marked_private, owner })
}

fn unicode_text() -> Result<Option<String>> {
    if unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)) }.is_err() {
        return Ok(None);
    }
    let data = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.context("reading clipboard text")?;
    let locked = LockedGlobal::lock(HGLOBAL(data.0))?;
    Ok(Some(text_before_nul(locked.utf16_units())))
}

fn dword_value(format: u32) -> Option<u32> {
    let data = unsafe { GetClipboardData(format) }.ok()?;
    let locked = LockedGlobal::lock(HGLOBAL(data.0)).ok()?;
    let bytes: [u8; 4] = locked.bytes().get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn owner() -> Option<ClipboardOwner> {
    let hwnd = unsafe { GetClipboardOwner() }.ok()?;
    let pid = owning_pid(hwnd);
    (pid != 0).then(|| ClipboardOwner { process: process_name(pid), title: title(hwnd) })
}

pub fn set_text(text: &str) -> Result<()> {
    replace_contents(vec![text_data(text)?])
}

pub fn set_text_transient(text: &str) -> Result<()> {
    let mut contents = history_exclusion_markers()?;
    contents.push(text_data(text)?);
    replace_contents(contents)
}

pub fn clear() -> Result<()> {
    let _clipboard = OpenClipboardGuard::open()?;
    unsafe { EmptyClipboard() }.context("emptying the clipboard")
}

pub fn holds_only_text() -> Result<bool> {
    let _clipboard = OpenClipboardGuard::open()?;
    let formats = clipboard_formats()?;
    Ok(formats.iter().all(|format| TEXT_FORMATS.contains(format)))
}

fn text_data(text: &str) -> Result<ClipboardData> {
    let memory = OwnedGlobal::with_bytes(&nul_terminated_utf16_bytes(text))?;
    Ok(ClipboardData { format: u32::from(CF_UNICODETEXT.0), memory })
}

fn nul_terminated_utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
}

fn replace_contents(contents: Vec<ClipboardData>) -> Result<()> {
    let _clipboard = OpenClipboardGuard::open()?;
    unsafe { EmptyClipboard() }.context("emptying the clipboard")?;
    let placed = contents.into_iter().try_for_each(place);
    if placed.is_err() {
        let _ = unsafe { EmptyClipboard() };
    }
    placed
}

fn place(data: ClipboardData) -> Result<()> {
    unsafe { SetClipboardData(data.format, Some(HANDLE(data.memory.handle().0))) }.context("writing clipboard data")?;
    let _owned_by_clipboard = data.memory.into_raw();
    Ok(())
}

fn clipboard_formats() -> Result<Vec<u32>> {
    let mut formats = Vec::new();
    let mut format = 0u32;
    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        formats.push(format);
    }
    let status = unsafe { GetLastError() };
    if status == NO_ERROR {
        Ok(formats)
    } else {
        Err(Error::internal(format!(
            "listing clipboard formats: {}",
            windows::core::Error::from_hresult(status.to_hresult())
        )))
    }
}

fn text_before_nul(units: &[u16]) -> String {
    let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_the_clipboard_works() {
        let _text = get_text().unwrap();
        let _only_text = holds_only_text().unwrap();
        let read = read().unwrap();
        assert!(!read.marked_private || read.text.is_none());
    }

    #[test]
    fn text_is_stored_as_nul_terminated_utf16() {
        assert_eq!(nul_terminated_utf16_bytes("hé"), vec![b'h', 0, 0xE9, 0, 0, 0]);
        assert_eq!(nul_terminated_utf16_bytes(""), vec![0, 0]);
    }

    #[test]
    fn text_stops_at_first_nul() {
        let units: Vec<u16> = "hi\0junk".encode_utf16().collect();
        assert_eq!(text_before_nul(&units), "hi");
        assert_eq!(text_before_nul(&[]), "");
    }

    #[test]
    #[ignore = "writes the clipboard"]
    fn set_then_get_round_trips() {
        let saved = get_text().unwrap();
        set_text("deckhand ✓ test").unwrap();
        assert_eq!(get_text().unwrap().as_deref(), Some("deckhand ✓ test"));
        assert!(holds_only_text().unwrap());
        set_text(saved.as_deref().unwrap_or_default()).unwrap();
    }

    #[test]
    #[ignore = "writes the clipboard"]
    fn transient_text_round_trips_with_history_markers() {
        let saved = get_text().unwrap();
        set_text_transient("deckhand transient").unwrap();
        assert_eq!(get_text().unwrap().as_deref(), Some("deckhand transient"));
        assert!(!holds_only_text().unwrap());
        let private = read().unwrap();
        assert!(private.marked_private && private.text.is_none());
        set_text(saved.as_deref().unwrap_or_default()).unwrap();
    }

    #[test]
    #[ignore = "writes the clipboard"]
    fn clear_leaves_the_clipboard_empty() {
        let saved = get_text().unwrap();
        clear().unwrap();
        assert_eq!(get_text().unwrap(), None);
        assert!(holds_only_text().unwrap());
        if let Some(text) = saved {
            set_text(&text).unwrap();
        }
    }
}
