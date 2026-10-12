use deckhand_core::{Context, Result};
use windows::{
    Win32::{Foundation::HINSTANCE, System::LibraryLoader::GetModuleHandleW},
    core::PCWSTR,
};

pub(crate) fn module_instance() -> Result<HINSTANCE> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("finding this module")?;
    Ok(module.into())
}
