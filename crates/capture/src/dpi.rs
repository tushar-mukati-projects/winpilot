use deckhand_core::{Error, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetWindowDpiAwarenessContext,
    SetThreadDpiAwarenessContext,
};

pub(crate) fn make_thread_per_monitor_aware() -> Result<()> {
    set_thread_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).map(|_| ())
}

pub(crate) fn with_window_dpi<T>(hwnd: HWND, work: impl FnOnce() -> Result<T>) -> Result<T> {
    let context = unsafe { GetWindowDpiAwarenessContext(hwnd) };
    if context.0.is_null() {
        return work();
    }
    let previous = set_thread_context(context)?;
    let outcome = work();
    set_thread_context(previous)?;
    outcome
}

fn set_thread_context(context: DPI_AWARENESS_CONTEXT) -> Result<DPI_AWARENESS_CONTEXT> {
    let previous = unsafe { SetThreadDpiAwarenessContext(context) };
    if previous.0.is_null() {
        return Err(Error::internal("SetThreadDpiAwarenessContext rejected the awareness context"));
    }
    Ok(previous)
}
