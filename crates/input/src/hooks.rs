use deckhand_core::{Context, INJECTION_MARK, Result};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CallNextHookEx, HC_ACTION, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
            MSLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOWS_HOOK_ID,
        },
    },
    core::PCWSTR,
};

use crate::{input_clock::record_user_input, message_thread::MessageLoop};

const ALL_THREADS: u32 = 0;

pub struct InputHooks {
    _mouse: Hook,
    _keyboard: Hook,
}

impl InputHooks {
    pub fn install() -> Result<Self> {
        let module = current_module()?;
        let mouse = Hook::install(WH_MOUSE_LL, Some(mouse_hook), module)?;
        let keyboard = Hook::install(WH_KEYBOARD_LL, Some(keyboard_hook), module)?;
        Ok(Self { _mouse: mouse, _keyboard: keyboard })
    }
}

impl MessageLoop for InputHooks {
    fn on_message(&mut self, _message: &MSG) {}
}

struct Hook(HHOOK);

impl Hook {
    fn install(kind: WINDOWS_HOOK_ID, procedure: HOOKPROC, module: HINSTANCE) -> Result<Self> {
        let handle = unsafe { SetWindowsHookExW(kind, procedure, Some(module), ALL_THREADS) }
            .context("installing a low-level input hook")?;
        Ok(Self(handle))
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
    }
}

fn current_module() -> Result<HINSTANCE> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("finding the current module")?;
    Ok(module.into())
}

pub fn is_user_input(injected: bool, extra_info: usize) -> bool {
    !(injected && extra_info == INJECTION_MARK)
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = unsafe { (lparam.0 as *const MSLLHOOKSTRUCT).as_ref() };
        if event.is_some_and(|e| is_user_input(e.flags & LLMHF_INJECTED != 0, e.dwExtraInfo)) {
            record_user_input();
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = unsafe { (lparam.0 as *const KBDLLHOOKSTRUCT).as_ref() };
        if event.is_some_and(|e| is_user_input(e.flags.contains(LLKHF_INJECTED), e.dwExtraInfo)) {
            record_user_input();
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_input_counts_as_the_user() {
        assert!(is_user_input(false, 0));
        assert!(is_user_input(false, INJECTION_MARK));
    }

    #[test]
    fn other_injectors_count_as_the_user() {
        assert!(is_user_input(true, 0));
        assert!(is_user_input(true, 0x1234));
    }

    #[test]
    fn our_own_marked_injection_is_ignored() {
        assert!(!is_user_input(true, INJECTION_MARK));
    }
}
