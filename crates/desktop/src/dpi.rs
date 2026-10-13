use windows::Win32::{
    Foundation::ERROR_ACCESS_DENIED,
    UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
};
use winpilot_core::{Context, Result};

pub fn enable_dpi_awareness() -> Result<()> {
    match unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ERROR_ACCESS_DENIED.to_hresult() => Ok(()),
        Err(error) => Err(error).context("enabling per-monitor DPI awareness"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabling_twice_is_fine() {
        enable_dpi_awareness().unwrap();
        enable_dpi_awareness().unwrap();
    }
}
