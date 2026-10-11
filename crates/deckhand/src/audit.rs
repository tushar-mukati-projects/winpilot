use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Days, Local, NaiveDate, SecondsFormat};
use deckhand_core::{ErrorCode, config::LogConfig};
use serde::Serialize;

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

impl Audit {
    pub fn open(config: &LogConfig) -> Self {
        let Some(dir) = default_dir() else {
            eprintln!("deckhand: LOCALAPPDATA is not set, so the audit log is off");
            return Self::disabled();
        };
        if let Err(error) = fs::create_dir_all(&dir) {
            eprintln!("deckhand: could not create the audit log folder {}: {error}", dir.display());
        }
        prune_quietly(&dir, Local::now().date_naive(), config.retention_days);
        Self::in_dir(dir)
    }

    pub fn in_dir(dir: PathBuf) -> Self {
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
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("deckhand").join("logs"))
}

fn file_name(date: NaiveDate) -> String {
    format!("{}.jsonl", date.format("%Y-%m-%d"))
}

pub fn log_date(name: &str) -> Option<NaiveDate> {
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

fn prune_quietly(dir: &Path, today: NaiveDate, retention_days: u32) {
    if let Err(error) = prune(dir, today, retention_days) {
        eprintln!("deckhand: could not clean old audit logs in {}: {error}", dir.display());
    }
}

pub fn prune(dir: &Path, today: NaiveDate, retention_days: u32) -> io::Result<usize> {
    let Some(cutoff) = today.checked_sub_days(Days::new(u64::from(retention_days))) else { return Ok(0) };
    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let expired = entry.file_name().to_str().and_then(log_date).is_some_and(|date| date < cutoff);
        if expired && entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
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
        let removed = prune(temp.path(), date("2026-10-11"), 14).unwrap();
        assert_eq!(removed, 1);
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
        let entry = Entry { tool: "status", summary: "", failure: None, elapsed: Duration::ZERO };
        let line = audit.line(&entry, Local::now()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&line).unwrap();
        let ts = value["ts"].as_str().unwrap();
        assert!(DateTime::parse_from_rfc3339(ts).is_ok(), "{ts}");
        assert!(!ts.ends_with('Z'), "{ts}");
    }

    #[test]
    fn disabled_audit_writes_nothing() {
        let entry = Entry { tool: "status", summary: "", failure: None, elapsed: Duration::ZERO };
        Audit::disabled().record(&entry);
    }
}
