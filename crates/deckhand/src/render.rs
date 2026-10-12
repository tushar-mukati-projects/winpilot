use std::time::Duration;

use deckhand_core::{
    Error, Result,
    format::{self, ObserveFormat},
    protocol::{ActReport, After, Reply, ScreenshotReport, StatusReport},
};
use rmcp::model::{CallToolResult, ContentBlock};

use crate::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extras<'a> {
    pub observe: ObserveFormat,
    pub server_uptime: Duration,
    pub restarts: u32,
    pub config_problem: Option<&'a str>,
    pub worker_problem: Option<&'a str>,
}

pub fn outcome(result: Result<Reply>, extras: &Extras) -> CallToolResult {
    match result {
        Ok(reply) => CallToolResult::success(contents(reply, extras)),
        Err(error) => failure(&error),
    }
}

fn failure(error: &Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format::error(error))])
}

fn contents(reply: Reply, extras: &Extras) -> Vec<ContentBlock> {
    match reply {
        Reply::Observation(observation) => text(format::observation(&observation, &extras.observe)),
        Reply::Search(report) => text(format::search(&report)),
        Reply::Text(report) => text(format::text(&report)),
        Reply::Screenshot(shot) => captioned(format::screenshot_caption(&shot), shot),
        Reply::Record(report) => text(format::record(&report)),
        Reply::Listen(report) => text(format::listen(&report)),
        Reply::Windows(report) => text(format::windows(&report)),
        Reply::Monitors(report) => text(format::monitors(&report)),
        Reply::Apps(report) => text(format::apps(&report)),
        Reply::Clipboard(report) => text(format::clipboard(&report)),
        Reply::Status(report) => text(status(report, extras)),
        Reply::Act(report) => act(report),
    }
}

fn act(report: ActReport) -> Vec<ContentBlock> {
    let summary = format::act(&report);
    match report.after {
        After::Screenshot(shot) => captioned(summary, shot),
        After::None | After::Diff(_) | After::Observation(_) => text(summary),
    }
}

fn status(report: StatusReport, extras: &Extras) -> String {
    let mut summary = format::status(&with_server_facts(report, extras));
    if let Some(problem) = extras.worker_problem {
        summary.push_str(&worker_line(problem));
    }
    if let Some(problem) = extras.config_problem {
        summary.push_str(&config_line(problem));
    }
    summary
}

fn with_server_facts(mut report: StatusReport, extras: &Extras) -> StatusReport {
    report.worker_restarts = extras.restarts;
    report.server_uptime_ms = Some(u64::try_from(extras.server_uptime.as_millis()).unwrap_or(u64::MAX));
    report
}

fn worker_line(problem: &str) -> String {
    format!("\nworker: {problem}")
}

fn config_line(problem: &str) -> String {
    format!("\nconfig: {} (every other tool is refused until then)", config::advice(problem))
}

fn text(body: String) -> Vec<ContentBlock> {
    vec![ContentBlock::text(body)]
}

fn captioned(caption: String, shot: ScreenshotReport) -> Vec<ContentBlock> {
    vec![ContentBlock::text(caption), ContentBlock::image(shot.data, shot.mime)]
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Point, geometry::ImageMapping, refs::ImageRef};

    use super::*;
    use crate::testing::status_report;

    #[test]
    fn captioned_puts_text_before_the_image() {
        let shot = ScreenshotReport {
            image: ImageRef(7),
            mime: "image/jpeg".to_string(),
            data: "QUJD".to_string(),
            width: 2,
            height: 1,
            mapping: ImageMapping { origin: Point::new(0, 0), scale: 1.0 },
            source: "dxgi".to_string(),
            quality: Some(80),
            note: None,
        };
        let blocks = captioned("img=i7".to_string(), shot);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].as_text().map(|text| text.text.as_str()), Some("img=i7"));
        let image = blocks[1].as_image().unwrap();
        assert_eq!((image.data.as_str(), image.mime_type.as_str()), ("QUJD", "image/jpeg"));
    }

    #[test]
    fn restarts_and_server_uptime_come_from_the_server() {
        let extras = Extras {
            observe: ObserveFormat::default(),
            server_uptime: Duration::from_secs(9),
            restarts: 3,
            config_problem: None,
            worker_problem: None,
        };
        let report = with_server_facts(status_report(), &extras);
        assert_eq!((report.worker_restarts, report.server_uptime_ms), (3, Some(9000)));
    }

    #[test]
    fn config_problem_is_one_extra_line() {
        assert_eq!(
            config_line("bad key"),
            "\nconfig: bad key; fix it or delete it to use the defaults (every other tool is refused until then)"
        );
    }

    #[test]
    fn worker_and_config_problems_follow_the_status_lines() {
        let extras = Extras {
            observe: ObserveFormat::default(),
            server_uptime: Duration::from_millis(2500),
            restarts: 4,
            config_problem: Some("bad key"),
            worker_problem: Some("it exited"),
        };
        let text = status(status_report(), &extras);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.contains(&"worker restarts: 4"), "{text}");
        assert_eq!(lines[1], "server uptime: 2.5 s");
        assert_eq!(lines[2], "worker uptime: 0.0 s");
        assert_eq!(lines[lines.len() - 2], "worker: it exited");
        assert!(lines[lines.len() - 1].starts_with("config: bad key; "), "{text}");
    }
}
