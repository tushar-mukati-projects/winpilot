use std::{marker::PhantomData, thread, time::Duration};

use deckhand_core::{Context, Error, ErrorCode, Result};
use windows::Win32::{
    Foundation::{GetLastError, HANDLE, HGLOBAL, NO_ERROR},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        Ole::{CF_LOCALE, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT},
    },
};

use crate::global_memory::{LockedGlobal, OwnedGlobal};

const OPEN_ATTEMPTS: u32 = 20;
const OPEN_RETRY_GAP: Duration = Duration::from_millis(10);
const TEXT_FORMATS: [u32; 4] = [CF_UNICODETEXT.0 as u32, CF_TEXT.0 as u32, CF_OEMTEXT.0 as u32, CF_LOCALE.0 as u32];

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
    if unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)) }.is_err() {
        return Ok(None);
    }
    let data = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.context("reading clipboard text")?;
    let locked = LockedGlobal::lock(HGLOBAL(data.0))?;
    Ok(Some(text_before_nul(locked.utf16_units())))
}

pub fn set_text(text: &str) -> Result<()> {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let memory = filled_global(&units)?;
    let _clipboard = OpenClipboardGuard::open()?;
    unsafe { EmptyClipboard() }.context("emptying the clipboard")?;
    unsafe { SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.handle().0))) }
        .context("writing clipboard text")?;
    let _owned_by_clipboard = memory.into_raw();
    Ok(())
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

fn filled_global(units: &[u16]) -> Result<OwnedGlobal> {
    let memory = OwnedGlobal::moveable(size_of_val(units))?;
    let mut locked = LockedGlobal::lock(memory.handle())?;
    let destination = locked.utf16_units_mut();
    let Some(prefix) = destination.get_mut(..units.len()) else {
        return Err(Error::internal("clipboard memory is smaller than requested"));
    };
    prefix.copy_from_slice(units);
    drop(locked);
    Ok(memory)
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
