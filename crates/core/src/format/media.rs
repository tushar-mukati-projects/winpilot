use crate::{
    geometry::Point,
    listening::ListenReport,
    recording::{RecordReport, RecordState},
    reports::ScreenshotReport,
};

use super::MS_PER_SECOND;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;
const BYTES_PER_KB: f64 = 1024.0;
const BYTES_PER_MB: f64 = 1024.0 * 1024.0;

pub fn screenshot_caption(report: &ScreenshotReport) -> String {
    let image = report.image;
    let format = report.mime.strip_prefix("image/").unwrap_or(&report.mime);
    let Point { x, y } = report.mapping.origin;
    let quality = report.quality.map(|quality| format!(" · q{quality}")).unwrap_or_default();
    let caption = format!(
        "{image} {}x{} {format}{quality} · scale {:.2} · origin {x},{y} · {} · click points as {{\"x\":..,\"y\":..,\"img\":\"{image}\"}}",
        report.width, report.height, report.mapping.scale, report.source
    );
    match &report.note {
        Some(note) => format!("{caption}\n{note}"),
        None => caption,
    }
}

pub fn listen(report: &ListenReport) -> String {
    let mut lines = vec![format!("heard {} · language {}", report.heard, report.language)];
    if report.segments.is_empty() && !report.truncated {
        lines.push("(no speech heard)".to_string());
    }
    lines.extend(report.segments.iter().map(|segment| format!("[{}] {}", timestamp(segment.start_ms), segment.text)));
    if report.truncated {
        lines.push("(truncated)".to_string());
    }
    lines.extend(report.notes.iter().cloned());
    lines.join(
        "
",
    )
}

fn timestamp(ms: u64) -> String {
    let seconds = ms / MS_PER_SECOND;
    if seconds >= SECONDS_PER_HOUR {
        return format!(
            "{}:{:02}:{:02}",
            seconds / SECONDS_PER_HOUR,
            seconds / SECONDS_PER_MINUTE % SECONDS_PER_MINUTE,
            seconds % SECONDS_PER_MINUTE
        );
    }
    format!("{:02}:{:02}", seconds / SECONDS_PER_MINUTE, seconds % SECONDS_PER_MINUTE)
}

pub fn record(report: &RecordReport) -> String {
    let mut lines = vec![record_headline(report)];
    lines.extend(report.note.clone());
    lines.join("\n")
}

fn record_headline(report: &RecordReport) -> String {
    let path = report.path.as_deref().unwrap_or("(no file)");
    match report.state {
        RecordState::Idle => "not recording".to_string(),
        RecordState::Starting => {
            "the recording is starting; check again with observe mode=record action=status".to_string()
        }
        RecordState::Recording => recording_line(report, path),
        RecordState::Finished => {
            let frames = frame_count(report.frames);
            let saved = format!("saved {path} · {:.1} s · {frames} · {}", report.seconds, byte_size(report.bytes));
            match report.audio {
                Some(audio) => format!("{saved} · {}", audio.label()),
                None => saved,
            }
        }
    }
}

fn recording_line(report: &RecordReport, path: &str) -> String {
    let mut parts =
        vec![format!("recording to {path}"), format!("{}x{} @ {} fps", report.width, report.height, report.fps)];
    parts.extend(report.audio.map(|audio| audio.label().to_string()));
    if report.frames > 0 {
        parts.push(format!("{:.1} s", report.seconds));
        parts.push(frame_count(report.frames));
    }
    parts.push("stop with observe mode=record action=stop".to_string());
    parts.join(" · ")
}

fn frame_count(frames: u64) -> String {
    if frames == 1 { "1 frame".to_string() } else { format!("{frames} frames") }
}

pub(super) fn byte_size(bytes: u64) -> String {
    let size = bytes as f64;
    if size >= BYTES_PER_MB {
        return format!("{:.1} MB", size / BYTES_PER_MB);
    }
    if size >= BYTES_PER_KB {
        return format!("{:.0} KB", size / BYTES_PER_KB);
    }
    format!("{bytes} B")
}
