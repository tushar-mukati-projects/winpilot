use std::cell::OnceCell;

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
    Unknown,
}

pub(crate) struct DesktopLookup {
    order: Vec<GUID>,
    current_id: Option<GUID>,
    current: Option<u32>,
    manager: OnceCell<Manager>,
}

struct Manager {
    manager: Option<IVirtualDesktopManager>,
    _apartment: Option<ComApartment>,
}

impl DesktopLookup {
    pub(crate) fn new() -> Self {
        let order = read_binary(DESKTOP_IDS).map(|bytes| parse_desktop_ids(&bytes)).unwrap_or_default();
        let current_id = read_binary(CURRENT_DESKTOP).and_then(|bytes| guid_from_bytes(&bytes));
        let current = current_id.and_then(|id| desktop_number(&order, id));
        Self { order, current_id, current, manager: OnceCell::new() }
    }

    pub(crate) fn placement(&self, hwnd: HWND) -> Placement {
        match cloak_of(cloak_flags(hwnd)) {
            Cloak::Shown => Placement::Current,
            Cloak::ByShell if self.order.len() <= 1 => Placement::Hidden,
            Cloak::ByShell => self.elsewhere(hwnd),
            Cloak::Other => Placement::Hidden,
        }
    }

    pub(crate) const fn current_number(&self) -> Option<u32> {
        self.current
    }

    fn elsewhere(&self, hwnd: HWND) -> Placement {
        let Some(manager) = self.manager() else { return Placement::Unknown };
        match self.current_id {
            Some(current) => self.compared(manager, hwnd, current),
            None => self.asked(manager, hwnd),
        }
    }

    fn compared(&self, manager: &IVirtualDesktopManager, hwnd: HWND, current: GUID) -> Placement {
        match unsafe { manager.GetWindowDesktopId(hwnd) } {
            Ok(desktop) if desktop == current => Placement::Hidden,
            Ok(desktop) if desktop != GUID::zeroed() => Placement::Elsewhere(desktop_number(&self.order, desktop)),
            _ => Placement::Unknown,
        }
    }

    fn asked(&self, manager: &IVirtualDesktopManager, hwnd: HWND) -> Placement {
        match unsafe { manager.IsWindowOnCurrentVirtualDesktop(hwnd) } {
            Ok(on_current) if on_current.as_bool() => Placement::Hidden,
            Ok(_) => self.numbered(manager, hwnd),
            Err(_) => Placement::Unknown,
        }
    }

    fn numbered(&self, manager: &IVirtualDesktopManager, hwnd: HWND) -> Placement {
        let desktop = unsafe { manager.GetWindowDesktopId(hwnd) }.ok().filter(|desktop| *desktop != GUID::zeroed());
        desktop.map_or(Placement::Unknown, |desktop| Placement::Elsewhere(desktop_number(&self.order, desktop)))
    }

    fn manager(&self) -> Option<&IVirtualDesktopManager> {
        self.manager.get_or_init(Manager::create).manager.as_ref()
    }
}

impl Manager {
    fn create() -> Self {
        let apartment = ComApartment::enter_any().ok();
        let manager = apartment.as_ref().and_then(|_| create_manager());
        Self { manager, _apartment: apartment }
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.manager = None;
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
        let lookup = DesktopLookup::new();
        for hwnd in top_level_windows().unwrap() {
            if cloak_flags(hwnd) == 0 {
                assert_eq!(lookup.placement(hwnd), Placement::Current);
            }
        }
    }

    #[test]
    fn the_virtual_desktop_manager_answers_for_live_windows() {
        let lookup = DesktopLookup::new();
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

    #[test]
    fn a_single_desktop_needs_no_virtual_desktop_manager() {
        let single = DesktopLookup {
            order: vec![GUID::from_u128(1)],
            current_id: Some(GUID::from_u128(1)),
            current: Some(1),
            manager: OnceCell::new(),
        };
        for hwnd in top_level_windows().unwrap() {
            if cloak_of(cloak_flags(hwnd)) == Cloak::ByShell {
                assert_eq!(single.placement(hwnd), Placement::Hidden);
            }
        }
        assert!(single.manager.get().is_none());
    }
}
