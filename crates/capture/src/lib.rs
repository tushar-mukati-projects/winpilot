mod actor;
mod canvas;
mod capture_area;
mod capturer;
mod com;
mod compose;
mod convert;
mod displays;
mod dpi;
mod duplication;
mod encode;
mod font;
mod frame;
mod freshness;
mod gdi;
mod marks;
mod monitor_list;
mod ocr;
mod output;
mod pixels;
mod recorder;
mod resize;
mod screen_grabber;
mod window;
mod window_print;
mod window_visibility;

pub use capturer::Capturer;
pub use encode::{EncodeOptions, Encoded, encode};
pub use frame::{Frame, FrameSource};
pub use marks::Mark;
pub use ocr::ocr;
pub use recorder::{
    COVER_HOLD, CoverHold, EndReason, Finished, OutputFile, Progress, Recorder, RecordingSource, RecordingSpec, Redact,
    Redactor, Started, StopCheck, VideoSettings,
};
