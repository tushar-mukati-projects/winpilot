use crate::{geometry::Rect, listening::ListenRequest, observe::OcrMode, quality::Quality, recording::RecordRequest};

pub(crate) const DEFAULT_ELEMENTS: usize = 300;
pub(crate) const DEFAULT_MATCHES: usize = 20;
pub(crate) const DEFAULT_APPS: usize = 30;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageFormat {
    #[default]
    Jpeg,
    Png,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserveRequest {
    Elements(ElementsRequest),
    Search(SearchRequest),
    Text(TextRequest),
    Screenshot(ScreenshotRequest),
    Record(RecordRequest),
    Listen(ListenRequest),
    Windows,
    Monitors,
    Apps(AppsRequest),
    Clipboard(ClipboardRequest),
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementsRequest {
    pub window: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardRequest {
    pub max_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub query: String,
    pub max: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextRequest {
    pub window: Option<String>,
    pub target: Option<String>,
    pub region: Option<Rect>,
    pub ocr: OcrMode,
    pub max_chars: Option<u32>,
    pub coords: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenshotRequest {
    pub target: Option<String>,
    pub region: Option<Rect>,
    pub monitor: Option<u32>,
    pub preset: Option<Quality>,
    pub max_width: Option<u32>,
    pub quality: Option<u8>,
    pub format: ImageFormat,
    pub marks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppsRequest {
    pub query: Option<String>,
    pub max: usize,
}
