use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, HFONT};
use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows::Win32::UI::WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS};
use windows::core::{Error, Owned, Result};

pub struct GuiFont(Owned<HFONT>);

impl GuiFont {
    pub fn for_dpi(dpi: u32) -> Result<Self> {
        let metrics = non_client_metrics(dpi)?;
        let font = unsafe { CreateFontIndirectW(&raw const metrics.lfMessageFont) };
        if font.is_invalid() {
            return Err(Error::new(E_FAIL, "CreateFontIndirectW failed for the message font"));
        }
        Ok(Self(unsafe { Owned::new(font) }))
    }

    pub fn handle(&self) -> HFONT {
        *self.0
    }
}

fn non_client_metrics(dpi: u32) -> Result<NONCLIENTMETRICSW> {
    let size = size_of::<NONCLIENTMETRICSW>() as u32;
    let mut metrics = NONCLIENTMETRICSW { cbSize: size, ..Default::default() };
    unsafe {
        SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, size, Some((&raw mut metrics).cast()), 0, dpi)?;
    }
    Ok(metrics)
}
