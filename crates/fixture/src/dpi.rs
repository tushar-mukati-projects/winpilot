use windows::Win32::Foundation::{E_HANDLE, HWND};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::core::{Error, Result};

pub const BASE_DPI: u32 = 96;

pub fn become_per_monitor_aware() -> Result<()> {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
}

pub fn window_dpi(window: HWND) -> Result<u32> {
    match unsafe { GetDpiForWindow(window) } {
        0 => Err(Error::new(E_HANDLE, "GetDpiForWindow returned 0")),
        dpi => Ok(dpi),
    }
}

pub fn scale(value: i32, dpi: u32) -> i32 {
    let base = i64::from(BASE_DPI);
    let scaled = (i64::from(value) * i64::from(dpi) + base / 2).div_euclid(base);
    scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_dpi_is_identity() {
        assert_eq!(scale(640, BASE_DPI), 640);
    }

    #[test]
    fn scales_common_factors() {
        assert_eq!(scale(640, 120), 800);
        assert_eq!(scale(640, 144), 960);
        assert_eq!(scale(640, 192), 1280);
    }

    #[test]
    fn rounds_to_nearest() {
        assert_eq!(scale(5, 120), 6);
        assert_eq!(scale(3, 120), 4);
        assert_eq!(scale(1, 144), 2);
    }

    #[test]
    fn extreme_values_saturate() {
        assert_eq!(scale(i32::MAX, 480), i32::MAX);
        assert_eq!(scale(i32::MIN, 480), i32::MIN);
    }
}
