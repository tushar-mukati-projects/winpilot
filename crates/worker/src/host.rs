use std::collections::HashSet;

use winpilot_core::{Error, ErrorCode, Result, WindowInfo};

use crate::{
    process_tree::{Ancestor, ProcessTable},
    window_classes::FRAME_HOST_PROCESS,
};

const SHELL_AND_SYSTEM: [&str; 12] = [
    "explorer.exe",
    "svchost.exe",
    "sihost.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "csrss.exe",
    "smss.exe",
    "RuntimeBroker.exe",
    "ShellExperienceHost.exe",
    "StartMenuExperienceHost.exe",
    FRAME_HOST_PROCESS,
];

#[derive(Debug, Clone, Default)]
pub(crate) struct HostProcesses(HashSet<u32>);

impl HostProcesses {
    pub(crate) fn detect() -> Self {
        let Ok(table) = ProcessTable::read() else { return Self::default() };
        let windowed: HashSet<u32> = winpilot_desktop::windows()
            .map(|windows| windows.iter().map(|window| window.pid).collect())
            .unwrap_or_default();
        Self(host_set(&table.ancestors(std::process::id()), &windowed))
    }

    pub(crate) fn contains(&self, pid: u32) -> bool {
        self.0.contains(&pid)
    }

    pub(crate) fn refuse(&self, window: &WindowInfo) -> Result<()> {
        if !self.contains(window.pid) {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::Denied,
            format!("{} is the agent's own terminal; WinPilot won't act on it", window.hwnd),
        ))
    }
}

fn host_set(ancestors: &[Ancestor], windowed: &HashSet<u32>) -> HashSet<u32> {
    let mut host = HashSet::new();
    for ancestor in ancestors {
        if is_shell_or_system(&ancestor.name) {
            return HashSet::new();
        }
        host.insert(ancestor.pid);
        if windowed.contains(&ancestor.pid) {
            return host;
        }
    }
    HashSet::new()
}

fn is_shell_or_system(name: &str) -> bool {
    SHELL_AND_SYSTEM.iter().any(|system| system.eq_ignore_ascii_case(name.trim()))
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn chain(entries: &[(u32, &str)]) -> Vec<Ancestor> {
        entries.iter().map(|&(pid, name)| Ancestor { pid, name: name.to_string() }).collect()
    }

    fn set(pids: &[u32]) -> HashSet<u32> {
        pids.iter().copied().collect()
    }

    #[test]
    fn the_host_ends_at_the_first_windowed_ancestor() {
        let walk = chain(&[(1, "winpilot.exe"), (2, "claude.exe"), (3, "notes.exe"), (4, "explorer.exe")]);
        assert_eq!(host_set(&walk, &set(&[3, 4])), set(&[1, 2, 3]));
    }

    #[test]
    fn explorer_and_system_processes_are_never_the_host() {
        let walk = chain(&[(1, "winpilot.exe"), (2, "claude.exe"), (4, "Explorer.EXE")]);
        assert!(host_set(&walk, &set(&[4])).is_empty());
        let service = chain(&[(1, "winpilot.exe"), (5, "svchost.exe"), (6, "terminal.exe")]);
        assert!(host_set(&service, &set(&[6])).is_empty());
    }

    #[test]
    fn without_a_windowed_ancestor_there_is_no_host() {
        let walk = chain(&[(1, "winpilot.exe"), (2, "claude.exe")]);
        assert!(host_set(&walk, &set(&[99])).is_empty());
        assert!(host_set(&[], &set(&[1])).is_empty());
    }

    #[test]
    fn host_windows_are_refused() {
        let host = HostProcesses(set(&[7]));
        let mut window = WindowInfo { pid: 7, ..window(42, "notes.exe", "Claude", Rect::new(0, 0, 10, 10)) };
        let error = host.refuse(&window).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
        assert_eq!(error.message, "w42 is the agent's own terminal; WinPilot won't act on it");
        window.pid = 8;
        assert!(host.refuse(&window).is_ok());
    }
}
