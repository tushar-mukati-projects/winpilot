mod actor;
mod audio;
mod canvas;
mod capture_area;
mod capturer;
mod compose;
mod d3d_device;
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
mod staging_texture;
mod time_units;
mod window;
mod window_print;
mod window_visibility;

pub use audio::{AudioFormat, AudioSource, Pcm, capture_audio};
pub use capturer::Capturer;
pub use encode::{EncodeOptions, Encoded, encode};
pub use frame::{Frame, FrameSource};
pub use marks::Mark;
pub use ocr::{OcrPass, ocr};
pub use recorder::{
    AudioTrack, COVER_HOLD, CoverHold, EndReason, Finished, OutputFile, Progress, Recorder, RecordingSource,
    RecordingSpec, Redact, Redactor, Started, StopCheck, VideoSettings,
};
