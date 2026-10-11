use crate::{
    element::{Element, WindowInfo},
    error::Error,
    protocol::{
        ActReport, ClipboardReport, FindReport, Observation, PickReport, ScreenshotReport, StatusReport, TextReport,
        WaitReport, WindowsReport,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObserveFormat {
    pub all: bool,
    pub coords: bool,
    pub max: usize,
}

impl Default for ObserveFormat {
    fn default() -> Self {
        Self { all: false, coords: false, max: 300 }
    }
}

pub fn observation(observation: &Observation, format: &ObserveFormat) -> String {
    todo!("{observation:?} {format:?}")
}

pub fn find(report: &FindReport) -> String {
    todo!("{report:?}")
}

pub fn act(report: &ActReport) -> String {
    todo!("{report:?}")
}

pub fn wait(report: &WaitReport) -> String {
    todo!("{report:?}")
}

pub fn windows(report: &WindowsReport) -> String {
    todo!("{report:?}")
}

pub fn pick(report: &PickReport) -> String {
    todo!("{report:?}")
}

pub fn text(report: &TextReport) -> String {
    todo!("{report:?}")
}

pub fn clipboard(report: &ClipboardReport) -> String {
    todo!("{report:?}")
}

pub fn status(report: &StatusReport) -> String {
    todo!("{report:?}")
}

pub fn screenshot_caption(report: &ScreenshotReport) -> String {
    todo!("{report:?}")
}

pub fn error(error: &Error) -> String {
    todo!("{error:?}")
}

pub fn window_line(window: &WindowInfo) -> String {
    todo!("{window:?}")
}

pub fn element_line(element: &Element, coords: bool) -> String {
    todo!("{element:?} {coords}")
}
