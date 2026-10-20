use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{ERROR_CLASS_ALREADY_EXISTS, HINSTANCE},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{RegisterClassExW, WNDCLASSEXW, WNDPROC},
    },
    core::PCWSTR,
};
use winpilot_core::{Context, Result};

pub fn register_window_class(
    instance: HINSTANCE,
    name: PCWSTR,
    procedure: WNDPROC,
    background: HBRUSH,
) -> windows::core::Result<()> {
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: procedure,
        hInstance: instance,
        hbrBackground: background,
        lpszClassName: name,
        ..WNDCLASSEXW::default()
    };
    if unsafe { RegisterClassExW(&raw const class) } != 0 {
        return Ok(());
    }
    let error = windows::core::Error::from_thread();
    if error.code() == ERROR_CLASS_ALREADY_EXISTS.to_hresult() { Ok(()) } else { Err(error) }
}

pub fn module_instance() -> Result<HINSTANCE> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("finding this module")?;
    Ok(module.into())
}
