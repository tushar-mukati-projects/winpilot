use std::{collections::HashMap, os::windows::io::OwnedHandle};

use windows::{
    Win32::{
        Foundation::ERROR_INSUFFICIENT_BUFFER,
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
    },
    core::PWSTR,
};

use crate::{
    handle::{adopt, raw},
    token::{own_integrity_level, process_integrity_level},
};

const UNKNOWN_PROCESS: &str = "?";
const IMAGE_PATH_CAPACITIES: [usize; 2] = [1024, 32_768];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessFacts {
    pub(crate) name: String,
    pub(crate) elevated: bool,
}

pub(crate) struct ProcessLookup {
    own_integrity: Option<u32>,
    known: HashMap<u32, ProcessFacts>,
}

impl ProcessLookup {
    pub(crate) fn new() -> Self {
        Self { own_integrity: own_integrity_level(), known: HashMap::new() }
    }

    pub(crate) fn facts(&mut self, pid: u32) -> ProcessFacts {
        let own_integrity = self.own_integrity;
        self.known.entry(pid).or_insert_with(|| inspect(pid, own_integrity)).clone()
    }
}

pub(crate) fn runs_above_own_integrity(pid: u32) -> bool {
    let Some(process) = open_for_query(pid) else { return true };
    outranks(process_integrity_level(raw(&process)), own_integrity_level())
}

fn inspect(pid: u32, own_integrity: Option<u32>) -> ProcessFacts {
    let Some(process) = open_for_query(pid) else {
        return ProcessFacts { name: UNKNOWN_PROCESS.to_string(), elevated: true };
    };
    let name = image_file_name(&process).unwrap_or_else(|| UNKNOWN_PROCESS.to_string());
    let elevated = outranks(process_integrity_level(raw(&process)), own_integrity);
    ProcessFacts { name, elevated }
}

fn outranks(target: Option<u32>, own: Option<u32>) -> bool {
    match (target, own) {
        (Some(target), Some(own)) => target > own,
        (None, _) => true,
        (Some(_), None) => false,
    }
}

pub(crate) fn open_for_query(pid: u32) -> Option<OwnedHandle> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    adopt(handle)
}

enum ImagePath {
    Found(String),
    BufferTooSmall,
    Unavailable,
}

fn image_file_name(process: &OwnedHandle) -> Option<String> {
    for capacity in IMAGE_PATH_CAPACITIES {
        match image_path(process, capacity) {
            ImagePath::Found(path) => return path.rsplit('\\').next().map(str::to_string),
            ImagePath::BufferTooSmall => {}
            ImagePath::Unavailable => return None,
        }
    }
    None
}

fn image_path(process: &OwnedHandle, capacity: usize) -> ImagePath {
    let mut buffer = vec![0u16; capacity];
    let mut length = capacity as u32;
    let queried = unsafe {
        QueryFullProcessImageNameW(raw(process), PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &raw mut length)
    };
    match queried {
        Ok(()) => ImagePath::Found(String::from_utf16_lossy(&buffer[..(length as usize).min(capacity)])),
        Err(error) if error.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() => ImagePath::BufferTooSmall,
        Err(_) => ImagePath::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_is_named_and_not_elevated_over_itself() {
        let mut lookup = ProcessLookup::new();
        let facts = lookup.facts(std::process::id());
        assert!(facts.name.to_ascii_lowercase().ends_with(".exe"));
        assert!(!facts.elevated);
    }

    #[test]
    fn own_process_does_not_outrank_itself_but_a_missing_one_does() {
        assert!(!runs_above_own_integrity(std::process::id()));
        assert!(runs_above_own_integrity(u32::MAX - 3));
    }

    #[test]
    fn missing_process_reads_as_unknown() {
        let mut lookup = ProcessLookup::new();
        let facts = lookup.facts(u32::MAX - 3);
        assert_eq!(facts.name, UNKNOWN_PROCESS);
        assert!(facts.elevated);
    }

    #[test]
    fn integrity_comparison() {
        assert!(outranks(Some(0x3000), Some(0x2000)));
        assert!(!outranks(Some(0x2000), Some(0x2000)));
        assert!(outranks(None, Some(0x2000)));
        assert!(!outranks(Some(0x3000), None));
    }
}
