use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HWND},
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RegGetValueW},
        },
        UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager},
    },
    core::{GUID, PCWSTR, w},
};

use crate::{
    com::ComApartment,
    desktop_ids::{Cloak, cloak_of, desktop_number, guid_from_bytes, parse_desktop_ids},
    window_visibility::cloak_flags,
};

const DESKTOPS_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");
const DESKTOP_IDS: PCWSTR = w!("VirtualDesktopIDs");
const CURRENT_DESKTOP: PCWSTR = w!("CurrentVirtualDesktop");
const MAX_REGISTRY_BYTES: u32 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Current,
    Elsewhere(Option<u32>),
    Hidden,
}

pub(crate) struct DesktopLookup {
    order: Vec<GUID>,
    current: Option<u32>,
    manager: Option<IVirtualDesktopManager>,
    tried_manager: bool,
    apartment: Option<ComApartment>,
}

impl DesktopLookup {
    pub(crate) fn new() -> Self {
        let order = read_binary(DESKTOP_IDS).map(|bytes| parse_desktop_ids(&bytes)).unwrap_or_default();
        let current_id = read_binary(CURRENT_DESKTOP).and_then(|bytes| guid_from_bytes(&bytes));
        let current = current_id.and_then(|id| desktop_number(&order, id));
        Self { order, current, manager: None, tried_manager: false, apartment: None }
    }

    pub(crate) fn placement(&mut self, hwnd: HWND) -> Placement {
        match cloak_of(cloak_flags(hwnd)) {
            Cloak::Shown => Placement::Current,
            Cloak::ByShell => self.elsewhere(hwnd),
            Cloak::Other => Placement::Hidden,
        }
    }

    pub(crate) const fn current_number(&self) -> Option<u32> {
        self.current
    }

    fn elsewhere(&mut self, hwnd: HWND) -> Placement {
        self.desktop_elsewhere(hwnd)
            .map_or(Placement::Hidden, |desktop| Placement::Elsewhere(desktop_number(&self.order, desktop)))
    }

    fn desktop_elsewhere(&mut self, hwnd: HWND) -> Option<GUID> {
        let manager = self.manager()?;
        let on_current = unsafe { manager.IsWindowOnCurrentVirtualDesktop(hwnd) }.ok()?;
        if on_current.as_bool() {
            return None;
        }
        let desktop = unsafe { manager.GetWindowDesktopId(hwnd) }.ok()?;
        (desktop != GUID::zeroed()).then_some(desktop)
    }

    fn manager(&mut self) -> Option<&IVirtualDesktopManager> {
        if !self.tried_manager {
            self.tried_manager = true;
            self.apartment = ComApartment::enter_mta().ok();
            self.manager = self.apartment.as_ref().and_then(|_| create_manager());
        }
        self.manager.as_ref()
    }
}

impl Drop for DesktopLookup {
    fn drop(&mut self) {
        self.manager = None;
        self.apartment = None;
    }
}

fn create_manager() -> Option<IVirtualDesktopManager> {
    unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) }.ok()
}

fn read_binary(value: PCWSTR) -> Option<Vec<u8>> {
    let mut size = 0u32;
    let sized = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, DESKTOPS_KEY, value, RRF_RT_REG_BINARY, None, None, Some(&raw mut size))
    };
    if sized != ERROR_SUCCESS || size == 0 || size > MAX_REGISTRY_BYTES {
        return None;
    }
    let mut bytes = vec![0u8; size as usize];
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            DESKTOPS_KEY,
            value,
            RRF_RT_REG_BINARY,
            None,
            Some(bytes.as_mut_ptr().cast()),
            Some(&raw mut size),
        )
    };
    if read != ERROR_SUCCESS {
        return None;
    }
    bytes.truncate(size as usize);
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{window_list::top_level_windows, window_visibility::is_visible};

    #[test]
    fn the_registry_names_at_most_as_many_desktops_as_it_lists() {
        let lookup = DesktopLookup::new();
        if let Some(current) = lookup.current_number() {
            assert!(current >= 1 && current as usize <= lookup.order.len());
        }
    }

    #[test]
    fn uncloaked_windows_are_on_the_current_desktop() {
        let mut lookup = DesktopLookup::new();
        for hwnd in top_level_windows().unwrap() {
            if cloak_flags(hwnd) == 0 {
                assert_eq!(lookup.placement(hwnd), Placement::Current);
            }
        }
    }

    #[test]
    fn the_virtual_desktop_manager_answers_for_live_windows() {
        let mut lookup = DesktopLookup::new();
        let manager = lookup.manager().cloned().unwrap();
        let windows = top_level_windows().unwrap();
        let answered = windows
            .iter()
            .filter(|hwnd| is_visible(**hwnd) && cloak_flags(**hwnd) == 0)
            .filter_map(|hwnd| unsafe { manager.IsWindowOnCurrentVirtualDesktop(*hwnd) }.ok())
            .collect::<Vec<_>>();
        assert!(answered.iter().all(|on_current| on_current.as_bool()));
        let placements: Vec<Placement> = windows.iter().map(|hwnd| lookup.placement(*hwnd)).collect();
        assert!(placements.contains(&Placement::Current));
    }
}
