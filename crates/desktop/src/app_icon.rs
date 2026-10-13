use windows::Win32::{
    Foundation::{FreeLibrary, HINSTANCE, HMODULE},
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    UI::{
        Controls::{_LI_METRIC, LIM_SMALL},
        WindowsAndMessaging::{CopyIcon, DestroyIcon, HICON, IDI_INFORMATION, LoadIconW},
    },
};
use windows::core::{HRESULT, PCWSTR, s, w};
use winpilot_core::{Context, Result};

use crate::module_instance::module_instance;

const EXE_ICON_RESOURCE_ID: usize = 1;

type LoadIconMetric = unsafe extern "system" fn(HINSTANCE, PCWSTR, _LI_METRIC, *mut HICON) -> HRESULT;
type AnyProcedure = unsafe extern "system" fn() -> isize;

#[derive(Debug)]
pub(crate) struct OwnedIcon(HICON);

struct Library(HMODULE);

impl OwnedIcon {
    pub(crate) fn small_app_icon() -> Result<Self> {
        if let Some(icon) = small_exe_icon() {
            return Ok(Self(icon));
        }
        let shared = unsafe { LoadIconW(None, IDI_INFORMATION) }.context("loading the information icon")?;
        let owned = unsafe { CopyIcon(shared) }.context("copying the information icon")?;
        Ok(Self(owned))
    }

    pub(crate) const fn handle(&self) -> HICON {
        self.0
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

impl Library {
    fn load(name: PCWSTR) -> Option<Self> {
        unsafe { LoadLibraryW(name) }.ok().map(Self)
    }

    fn load_icon_metric(&self) -> Option<LoadIconMetric> {
        let procedure = unsafe { GetProcAddress(self.0, s!("LoadIconMetric")) }?;
        Some(unsafe { std::mem::transmute::<AnyProcedure, LoadIconMetric>(procedure) })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = unsafe { FreeLibrary(self.0) };
    }
}

fn small_exe_icon() -> Option<HICON> {
    let common_controls = Library::load(w!("comctl32.dll"))?;
    let load_icon_metric = common_controls.load_icon_metric()?;
    let module = module_instance().ok()?;
    let resource = PCWSTR(std::ptr::without_provenance(EXE_ICON_RESOURCE_ID));
    let mut icon = HICON::default();
    unsafe { load_icon_metric(module, resource, LIM_SMALL, &raw mut icon) }.ok().ok()?;
    (!icon.is_invalid()).then_some(icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_icon_loads_even_without_the_exe_resource() {
        let icon = OwnedIcon::small_app_icon().unwrap();
        assert!(!icon.handle().is_invalid());
    }
}
