use std::{
    fs::{self, DirEntry, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Days, Local, NaiveDate, SecondsFormat};
use deckhand_core::ErrorCode;
use serde::Serialize;

use crate::app_data;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry<'a> {
    pub tool: &'a str,
    pub summary: &'a str,
    pub failure: Option<ErrorCode>,
    pub elapsed: Duration,
}

#[derive(Serialize)]
struct Line<'a> {
    ts: String,
    pid: u32,
    tool: &'a str,
    summary: &'a str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<ErrorCode>,
    ms: u64,
}

#[derive(Debug, Clone)]
pub struct Audit {
    dir: Option<PathBuf>,
    pid: u32,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Pruned {
    removed: usize,
    failures: Vec<String>,
}

impl Audit {
    pub fn open() -> Self {
        let Some(dir) = default_dir() else {
            eprintln!("deckhand: LOCALAPPDATA is not an absolute path, so the audit log is off");
            return Self::disabled();
        };
        if let Err(error) = fs::create_dir_all(&dir) {
            eprintln!("deckhand: could not create the audit log folder {}: {error}", dir.display());
        }
        Self::in_dir(dir)
    }

    pub fn prune_expired(&self, retention_days: u32) {
        let Some(dir) = &self.dir else { return };
        report_pruning(dir, prune(dir, Local::now().date_naive(), retention_days));
    }

    fn in_dir(dir: PathBuf) -> Self {
        Self { dir: Some(dir), pid: std::process::id() }
    }

    pub fn disabled() -> Self {
        Self { dir: None, pid: std::process::id() }
    }

    pub fn record(&self, entry: &Entry) {
        let Some(dir) = &self.dir else { return };
        if let Err(error) = self.append(dir, entry, Local::now()) {
            eprintln!("deckhand: could not write the audit log: {error}");
        }
    }

    fn append(&self, dir: &Path, entry: &Entry, now: DateTime<Local>) -> io::Result<()> {
        let line = self.line(entry, now)?;
        fs::create_dir_all(dir)?;
        let mut file = OpenOptions::new().create(true).append(true).open(dir.join(file_name(now.date_naive())))?;
        let written = file.write(&line)?;
        if written == line.len() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::WriteZero, format!("wrote {written} of {} bytes", line.len())))
        }
    }

    fn line(&self, entry: &Entry, now: DateTime<Local>) -> io::Result<Vec<u8>> {
        let line = Line {
            ts: now.to_rfc3339_opts(SecondsFormat::Millis, false),
            pid: self.pid,
            tool: entry.tool,
            summary: entry.summary,
            ok: entry.failure.is_none(),
            code: entry.failure,
            ms: u64::try_from(entry.elapsed.as_millis()).unwrap_or(u64::MAX),
        };
        let mut bytes = serde_json::to_vec(&line).map_err(io::Error::other)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

fn default_dir() -> Option<PathBuf> {
    app_data::dir_from_env("LOCALAPPDATA").map(|dir| dir.join("deckhand").join("logs"))
}

fn file_name(date: NaiveDate) -> String {
    format!("{}.jsonl", date.format("%Y-%m-%d"))
}

fn log_date(name: &str) -> Option<NaiveDate> {
    let stem = name.strip_suffix(".jsonl")?;
    let shaped = stem.len() == 10
        && stem.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if !shaped {
        return None;
    }
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

fn report_pruning(dir: &Path, outcome: io::Result<Pruned>) {
    match outcome {
        Ok(pruned) => {
            for failure in pruned.failures {
                eprintln!("deckhand: could not delete an old audit log: {failure}");
            }
        }
        Err(error) => eprintln!("deckhand: could not clean old audit logs in {}: {error}", dir.display()),
    }
}

fn prune(dir: &Path, today: NaiveDate, retention_days: u32) -> io::Result<Pruned> {
    let mut pruned = Pruned::default();
    let Some(cutoff) = today.checked_sub_days(Days::new(u64::from(retention_days))) else { return Ok(pruned) };
    for entry in fs::read_dir(dir)? {
        match entry.and_then(|entry| remove_if_expired(&entry, cutoff)) {
            Ok(removed) => pruned.removed += usize::from(removed),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => pruned.failures.push(error.to_string()),
        }
    }
    Ok(pruned)
}

fn remove_if_expired(entry: &DirEntry, cutoff: NaiveDate) -> io::Result<bool> {
    let expired = entry.file_name().to_str().and_then(log_date).is_some_and(|date| date < cutoff);
    if !expired || !entry.file_type()?.is_file() {
        return Ok(false);
    }
    let path = entry.path();
    fs::remove_file(&path).map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::os::windows::fs::OpenOptionsExt;

    use super::*;
    use crate::testing::TempDir;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn log_names_must_match_the_date_pattern_exactly() {
        assert_eq!(log_date("2026-10-11.jsonl"), Some(date("2026-10-11")));
        assert_eq!(log_date("2026-10-11.json"), None);
        assert_eq!(log_date("2026-10-11.jsonl.bak"), None);
        assert_eq!(log_date("x2026-10-11.jsonl"), None);
        assert_eq!(log_date("2026-1-11.jsonl"), None);
        assert_eq!(log_date("2026_10_11.jsonl"), None);
        assert_eq!(log_date("2026-13-40.jsonl"), None);
        assert_eq!(log_date("notes.jsonl"), None);
        assert_eq!(log_date("２０２６-10-11.jsonl"), None);
    }

    #[test]
    fn prune_removes_only_expired_dated_logs() {
        let temp = TempDir::new("audit-prune");
        let names = [
            "2026-09-01.jsonl",
            "2026-09-27.jsonl",
            "2026-10-11.jsonl",
            "keep.jsonl",
            "2026-09-01.txt",
            "2026-9-1.jsonl",
        ];
        for name in names {
            fs::write(temp.path().join(name), "{}\n").unwrap();
        }
        fs::create_dir(temp.path().join("2026-08-01.jsonl")).unwrap();
        let pruned = prune(temp.path(), date("2026-10-11"), 14).unwrap();
        assert_eq!(pruned, Pruned { removed: 1, failures: Vec::new() });
        let mut left: Vec<String> =
            fs::read_dir(temp.path()).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(
            left,
            [
                "2026-08-01.jsonl",
                "2026-09-01.txt",
                "2026-09-27.jsonl",
                "2026-10-11.jsonl",
                "2026-9-1.jsonl",
                "keep.jsonl"
            ]
        );
    }

    #[test]
    fn prune_keeps_going_past_a_file_it_cannot_delete() {
        let temp = TempDir::new("audit-prune-locked");
        let names = ["2026-09-01.jsonl", "2026-09-02.jsonl", "2026-09-03.jsonl"];
        for name in names {
            fs::write(temp.path().join(name), "{}\n").unwrap();
        }
        let locked = OpenOptions::new().read(true).share_mode(0).open(temp.path().join(names[1])).unwrap();
        let pruned = prune(temp.path(), date("2026-10-11"), 14).unwrap();
        assert_eq!(pruned.removed, 2);
        assert_eq!(pruned.failures.len(), 1);
        assert!(pruned.failures[0].contains(names[1]), "{}", pruned.failures[0]);
        drop(locked);
        let left: Vec<String> =
            fs::read_dir(temp.path()).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        assert_eq!(left, [names[1]]);
    }

    #[test]
    fn prune_of_a_missing_folder_is_an_error_not_a_panic() {
        let temp = TempDir::new("audit-prune-missing");
        assert!(prune(&temp.path().join("absent"), date("2026-10-11"), 14).is_err());
    }

    #[test]
    fn record_appends_one_json_line_per_call() {
        let temp = TempDir::new("audit-record");
        let audit = Audit::in_dir(temp.path().join("logs"));
        let ok = Entry { tool: "observe", summary: "", failure: None, elapsed: Duration::from_millis(42) };
        let failed = Entry {
            tool: "act",
            summary: "click e1 return=diff",
            failure: Some(ErrorCode::Timeout),
            elapsed: Duration::from_millis(7),
        };
        audit.record(&ok);
        audit.record(&failed);
        let files: Vec<PathBuf> =
            fs::read_dir(temp.path().join("logs")).unwrap().map(|entry| entry.unwrap().path()).collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().unwrap().to_str().unwrap().to_string();
        assert!(log_date(&name).is_some(), "{name}");
        let text = fs::read_to_string(&files[0]).unwrap();
        let lines: Vec<serde_json::Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["tool"], "observe");
        assert_eq!(lines[0]["ok"], true);
        assert_eq!(lines[0]["ms"], 42);
        assert!(lines[0].get("code").is_none());
        assert_eq!(lines[0]["pid"], std::process::id());
        assert_eq!(lines[1]["ok"], false);
        assert_eq!(lines[1]["code"], "timeout");
        assert_eq!(lines[1]["summary"], "click e1 return=diff");
    }

    #[test]
    fn timestamps_are_rfc3339_with_offset() {
        let audit = Audit::disabled();
        let entry = Entry { tool: "observe", summary: "mode=status", failure: None, elapsed: Duration::ZERO };
        let line = audit.line(&entry, Local::now()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&line).unwrap();
        let ts = value["ts"].as_str().unwrap();
        assert!(DateTime::parse_from_rfc3339(ts).is_ok(), "{ts}");
        assert!(!ts.ends_with('Z'), "{ts}");
    }

    #[test]
    fn disabled_audit_writes_nothing() {
        let entry = Entry { tool: "observe", summary: "mode=status", failure: None, elapsed: Duration::ZERO };
        Audit::disabled().record(&entry);
    }
}
