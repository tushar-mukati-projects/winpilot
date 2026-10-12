use std::collections::{HashMap, HashSet};

use deckhand_core::{Context, Error, ErrorCode, Result, WindowInfo};
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        },
        Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

use crate::window_classes::FRAME_HOST_PROCESS;

const MAX_DEPTH: usize = 32;
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
pub struct HostProcesses(HashSet<u32>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessEntry {
    parent: u32,
    created: u64,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ancestor {
    pub pid: u32,
    pub name: String,
}

impl HostProcesses {
    pub fn detect() -> Self {
        let Ok(table) = process_table() else { return Self::default() };
        let windowed: HashSet<u32> = deckhand_desktop::windows()
            .map(|windows| windows.iter().map(|window| window.pid).collect())
            .unwrap_or_default();
        Self(host_set(&ancestors(std::process::id(), &table), &windowed))
    }

    pub fn contains(&self, pid: u32) -> bool {
        self.0.contains(&pid)
    }

    pub fn refuse(&self, window: &WindowInfo) -> Result<()> {
        if !self.contains(window.pid) {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::Denied,
            format!("{} is the agent's own terminal; Deckhand won't act on it", window.hwnd),
        ))
    }
}

pub fn host_set(ancestors: &[Ancestor], windowed: &HashSet<u32>) -> HashSet<u32> {
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

fn ancestors(start: u32, table: &HashMap<u32, ProcessEntry>) -> Vec<Ancestor> {
    let mut chain: Vec<Ancestor> = Vec::new();
    let mut current = start;
    while chain.len() < MAX_DEPTH {
        let Some(entry) = table.get(&current) else { break };
        let Some(parent) = table.get(&entry.parent) else { break };
        let reused_pid = parent.created > entry.created;
        let seen = chain.iter().any(|ancestor| ancestor.pid == entry.parent);
        if entry.parent == 0 || entry.parent == start || reused_pid || seen {
            break;
        }
        chain.push(Ancestor { pid: entry.parent, name: parent.name.clone() });
        current = entry.parent;
    }
    chain
}

struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn process_table() -> Result<HashMap<u32, ProcessEntry>> {
    let snapshot = Snapshot(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.context("listing processes")?);
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut table = HashMap::new();
    let mut more = unsafe { Process32FirstW(snapshot.0, &raw mut entry) }.is_ok();
    while more {
        let created = process_creation_time(entry.th32ProcessID).unwrap_or(0);
        let name = exe_name(&entry.szExeFile);
        table.insert(entry.th32ProcessID, ProcessEntry { parent: entry.th32ParentProcessID, created, name });
        more = unsafe { Process32NextW(snapshot.0, &raw mut entry) }.is_ok();
    }
    Ok(table)
}

fn exe_name(units: &[u16]) -> String {
    let length = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..length])
}

fn process_creation_time(pid: u32) -> Option<u64> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let times = unsafe { GetProcessTimes(process, &raw mut created, &raw mut exited, &raw mut kernel, &raw mut user) };
    let _ = unsafe { CloseHandle(process) };
    times.ok()?;
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Rect, WindowRef, WindowState};

    use super::*;

    fn table(entries: &[(u32, u32, u64)]) -> HashMap<u32, ProcessEntry> {
        entries
            .iter()
            .map(|&(pid, parent, created)| (pid, ProcessEntry { parent, created, name: format!("p{pid}.exe") }))
            .collect()
    }

    fn pids(chain: &[Ancestor]) -> Vec<u32> {
        chain.iter().map(|ancestor| ancestor.pid).collect()
    }

    fn chain(entries: &[(u32, &str)]) -> Vec<Ancestor> {
        entries.iter().map(|&(pid, name)| Ancestor { pid, name: name.to_string() }).collect()
    }

    fn set(pids: &[u32]) -> HashSet<u32> {
        pids.iter().copied().collect()
    }

    #[test]
    fn walks_up_to_the_root_nearest_first() {
        let processes = table(&[(10, 20, 5), (20, 30, 4), (30, 0, 1)]);
        let found = ancestors(10, &processes);
        assert_eq!(pids(&found), [20, 30]);
        assert_eq!(found[0].name, "p20.exe");
    }

    #[test]
    fn stops_at_a_reused_parent_pid() {
        let processes = table(&[(10, 20, 5), (20, 30, 9), (30, 0, 1)]);
        assert!(ancestors(10, &processes).is_empty());
    }

    #[test]
    fn stops_on_cycles_and_unknown_parents() {
        let cyclic = table(&[(10, 20, 5), (20, 10, 5)]);
        assert_eq!(pids(&ancestors(10, &cyclic)), [20]);
        let orphan = table(&[(10, 99, 5)]);
        assert!(ancestors(10, &orphan).is_empty());
    }

    #[test]
    fn the_host_ends_at_the_first_windowed_ancestor() {
        let walk = chain(&[(1, "deckhand.exe"), (2, "claude.exe"), (3, "notes.exe"), (4, "explorer.exe")]);
        assert_eq!(host_set(&walk, &set(&[3, 4])), set(&[1, 2, 3]));
    }

    #[test]
    fn explorer_and_system_processes_are_never_the_host() {
        let walk = chain(&[(1, "deckhand.exe"), (2, "claude.exe"), (4, "Explorer.EXE")]);
        assert!(host_set(&walk, &set(&[4])).is_empty());
        let service = chain(&[(1, "deckhand.exe"), (5, "svchost.exe"), (6, "terminal.exe")]);
        assert!(host_set(&service, &set(&[6])).is_empty());
    }

    #[test]
    fn without_a_windowed_ancestor_there_is_no_host() {
        let walk = chain(&[(1, "deckhand.exe"), (2, "claude.exe")]);
        assert!(host_set(&walk, &set(&[99])).is_empty());
        assert!(host_set(&[], &set(&[1])).is_empty());
    }

    #[test]
    fn host_windows_are_refused() {
        let host = HostProcesses(set(&[7]));
        let mut window = WindowInfo {
            hwnd: WindowRef(42),
            title: "Claude".to_string(),
            process: "notes.exe".to_string(),
            pid: 7,
            class_name: "Notes".to_string(),
            rect: Rect::new(0, 0, 10, 10),
            state: WindowState::Normal,
            foreground: true,
            elevated: false,
            desktop: None,
            on_current_desktop: true,
        };
        let error = host.refuse(&window).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
        assert_eq!(error.message, "w42 is the agent's own terminal; Deckhand won't act on it");
        window.pid = 8;
        assert!(host.refuse(&window).is_ok());
    }

    #[test]
    fn process_names_stop_at_the_terminator() {
        let mut units = [0u16; 8];
        units[..3].copy_from_slice(&[u16::from(b'a'), u16::from(b'.'), u16::from(b'x')]);
        assert_eq!(exe_name(&units), "a.x");
    }
}
