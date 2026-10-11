use deckhand_core::{
    Error, Result,
    format::{self, ObserveFormat},
    protocol::{ActReport, After, Reply, ScreenshotReport, StatusReport},
};
use rmcp::model::{CallToolResult, ContentBlock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extras<'a> {
    pub observe: ObserveFormat,
    pub restarts: u32,
    pub config_problem: Option<&'a str>,
}

pub fn outcome(result: Result<Reply>, extras: &Extras) -> CallToolResult {
    match result {
        Ok(reply) => CallToolResult::success(contents(reply, extras)),
        Err(error) => failure(&error),
    }
}

pub fn failure(error: &Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format::error(error))])
}

fn contents(reply: Reply, extras: &Extras) -> Vec<ContentBlock> {
    match reply {
        Reply::Observe(observation) => text(format::observation(&observation, &extras.observe)),
        Reply::Find(report) => text(format::find(&report)),
        Reply::Screenshot(shot) => captioned(format::screenshot_caption(&shot), shot),
        Reply::Act(report) => act(report),
        Reply::WaitFor(report) => text(format::wait(&report)),
        Reply::Windows(report) => text(format::windows(&report)),
        Reply::PickFile(report) => text(format::pick(&report)),
        Reply::ReadText(report) => text(format::text(&report)),
        Reply::Clipboard(report) => text(format::clipboard(&report)),
        Reply::Status(report) => text(status(report, extras)),
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
    let mut summary = format::status(&with_restarts(report, extras.restarts));
    if let Some(problem) = extras.config_problem {
        summary.push_str(&config_line(problem));
    }
    summary
}

fn with_restarts(mut report: StatusReport, restarts: u32) -> StatusReport {
    report.worker_restarts = restarts;
    report
}

fn config_line(problem: &str) -> String {
    format!("\nconfig: {problem} (using default settings)")
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
        };
        let blocks = captioned("img=i7".to_string(), shot);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].as_text().map(|text| text.text.as_str()), Some("img=i7"));
        let image = blocks[1].as_image().unwrap();
        assert_eq!((image.data.as_str(), image.mime_type.as_str()), ("QUJD", "image/jpeg"));
    }

    #[test]
    fn status_restarts_come_from_the_supervisor() {
        let report = status_report();
        assert_eq!(with_restarts(report, 3).worker_restarts, 3);
    }

    #[test]
    fn config_problem_is_one_extra_line() {
        assert_eq!(config_line("bad key"), "\nconfig: bad key (using default settings)");
    }
}
