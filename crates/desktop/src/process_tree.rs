use std::{collections::HashMap, os::windows::io::OwnedHandle};

use windows::Win32::{
    Foundation::FILETIME,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        },
        SystemInformation::GetSystemTimeAsFileTime,
        Threading::GetProcessTimes,
    },
};
use winpilot_core::{Context, Error, Result};
use winpilot_win32::{adopt, raw};

use crate::process::open_for_query;

const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessTable(HashMap<u32, ProcessEntry>);

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

impl ProcessTable {
    pub fn read() -> Result<Self> {
        let snapshot = process_snapshot()?;
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut table = HashMap::new();
        let mut more = unsafe { Process32FirstW(raw(&snapshot), &raw mut entry) }.is_ok();
        while more {
            let created = process_creation_time(entry.th32ProcessID).unwrap_or(0);
            let name = exe_name(&entry.szExeFile);
            table.insert(entry.th32ProcessID, ProcessEntry { parent: entry.th32ParentProcessID, created, name });
            more = unsafe { Process32NextW(raw(&snapshot), &raw mut entry) }.is_ok();
        }
        Ok(Self(table))
    }

    pub fn ancestors(&self, start: u32) -> Vec<Ancestor> {
        let mut chain: Vec<Ancestor> = Vec::new();
        let mut current = start;
        while chain.len() < MAX_DEPTH {
            let Some(entry) = self.0.get(&current) else { break };
            let Some(parent) = self.0.get(&entry.parent) else { break };
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

    pub fn descends_from(&self, pid: u32, root: u32, started_since: u64) -> bool {
        let mut current = pid;
        for _ in 0..MAX_DEPTH {
            let Some(entry) = self.0.get(&current) else { return false };
            if entry.created < started_since {
                return false;
            }
            if entry.parent == root {
                return true;
            }
            if entry.parent == 0 || entry.parent == current {
                return false;
            }
            current = entry.parent;
        }
        false
    }
}

pub fn system_time_now() -> u64 {
    filetime_value(unsafe { GetSystemTimeAsFileTime() })
}

fn process_snapshot() -> Result<OwnedHandle> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.context("listing processes")?;
    unsafe { adopt(snapshot) }.ok_or_else(|| Error::internal("listing processes: Windows returned no snapshot"))
}

fn exe_name(units: &[u16]) -> String {
    let length = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..length])
}

fn process_creation_time(pid: u32) -> Option<u64> {
    let process = open_for_query(pid)?;
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(raw(&process), &raw mut created, &raw mut exited, &raw mut kernel, &raw mut user) }
        .ok()?;
    Some(filetime_value(created))
}

const fn filetime_value(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: &[(u32, u32, u64)]) -> ProcessTable {
        ProcessTable(
            entries
                .iter()
                .map(|&(pid, parent, created)| (pid, ProcessEntry { parent, created, name: format!("p{pid}.exe") }))
                .collect(),
        )
    }

    fn pids(chain: &[Ancestor]) -> Vec<u32> {
        chain.iter().map(|ancestor| ancestor.pid).collect()
    }

    #[test]
    fn walks_up_to_the_root_nearest_first() {
        let processes = table(&[(10, 20, 5), (20, 30, 4), (30, 0, 1)]);
        let found = processes.ancestors(10);
        assert_eq!(pids(&found), [20, 30]);
        assert_eq!(found[0].name, "p20.exe");
    }

    #[test]
    fn stops_at_a_reused_parent_pid() {
        let processes = table(&[(10, 20, 5), (20, 30, 9), (30, 0, 1)]);
        assert!(processes.ancestors(10).is_empty());
    }

    #[test]
    fn stops_on_cycles_and_unknown_parents() {
        let cyclic = table(&[(10, 20, 5), (20, 10, 5)]);
        assert_eq!(pids(&cyclic.ancestors(10)), [20]);
        let orphan = table(&[(10, 99, 5)]);
        assert!(orphan.ancestors(10).is_empty());
    }

    #[test]
    fn children_and_grandchildren_of_a_launch_descend_from_it_even_after_it_exits() {
        let processes = table(&[(40, 30, 120), (41, 40, 130), (50, 7, 125)]);
        assert!(processes.descends_from(40, 30, 100));
        assert!(processes.descends_from(41, 30, 100));
        assert!(!processes.descends_from(50, 30, 100));
    }

    #[test]
    fn processes_older_than_the_launch_never_descend_from_it() {
        let reused = table(&[(40, 30, 90)]);
        assert!(!reused.descends_from(40, 30, 100));
        let stale_link = table(&[(41, 40, 130), (40, 30, 90)]);
        assert!(!stale_link.descends_from(41, 30, 100));
        let unknown_age = table(&[(42, 30, 0)]);
        assert!(!unknown_age.descends_from(42, 30, 100));
    }

    #[test]
    fn unknown_processes_and_loops_end_the_walk() {
        assert!(!table(&[]).descends_from(40, 30, 0));
        assert!(!table(&[(40, 40, 120)]).descends_from(40, 30, 100));
        assert!(!table(&[(40, 41, 120), (41, 40, 120)]).descends_from(40, 30, 100));
        assert!(!table(&[(40, 0, 120)]).descends_from(40, 30, 100));
    }

    #[test]
    fn process_names_stop_at_the_terminator() {
        let mut units = [0u16; 8];
        units[..3].copy_from_slice(&[u16::from(b'a'), u16::from(b'.'), u16::from(b'x')]);
        assert_eq!(exe_name(&units), "a.x");
    }

    #[test]
    fn file_times_join_their_halves() {
        assert_eq!(filetime_value(FILETIME { dwLowDateTime: 2, dwHighDateTime: 1 }), (1 << 32) | 2);
        assert!(system_time_now() > 0);
    }
}
