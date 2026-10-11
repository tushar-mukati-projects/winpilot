use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    blank::non_blank,
    error::{Error, Result},
    format::ObserveFormat,
    geometry::Rect,
    observe_fields::check_fields,
    observe_request::{
        AppsRequest, ClipboardRequest, DEFAULT_APPS, DEFAULT_ELEMENTS, DEFAULT_MATCHES, ElementsRequest, ImageFormat,
        ObserveRequest, ScreenshotRequest, SearchRequest, TextRequest,
    },
    quality::Quality,
    recording::{RecordAction, RecordRequest, VideoFormat},
    text_limit::char_limit,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObserveMode {
    #[default]
    Elements,
    Text,
    Screenshot,
    Record,
    Windows,
    Monitors,
    Apps,
    Clipboard,
    Status,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    #[default]
    Auto,
    Never,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaFormat {
    Jpeg,
    Png,
    Mp4,
    Gif,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ObserveArgs {
    #[schemars(
        description = "elements (default), text, screenshot, record, windows, monitors, apps, clipboard or status."
    )]
    pub mode: ObserveMode,
    #[schemars(
        description = "elements, text: the window, as a ref, title text or window:/process: selector. Default: the default window."
    )]
    pub window: Option<String>,
    #[schemars(
        description = "text, screenshot, record: an element or window ref or selector; an element crops the capture to it."
    )]
    pub target: Option<String>,
    #[schemars(description = "elements: selector filter (matches plus their ancestors). apps: name filter.")]
    pub query: Option<String>,
    #[schemars(
        description = "elements: search every app window with query; returns match lines with window and path."
    )]
    pub all_windows: Option<bool>,
    #[schemars(description = "elements: include non-interactive elements such as static text and panes.")]
    pub all: Option<bool>,
    #[schemars(description = "elements: add @x,y,wxh screen rectangles.")]
    pub coords: Option<bool>,
    #[schemars(description = "elements, apps: cap on lines (default 300), all_windows matches (20) or apps (30).")]
    pub max: Option<u32>,
    #[schemars(description = "text, screenshot, record: a screen rectangle in physical pixels.")]
    pub region: Option<Rect>,
    #[schemars(description = "screenshot, record: a monitor index (0 = primary, as mode=monitors lists them).")]
    pub monitor: Option<u32>,
    #[schemars(description = "text: auto (default: UI Automation text, OCR as fallback), never or always.")]
    pub ocr: Option<OcrMode>,
    #[schemars(description = "text, clipboard: maximum characters (default 20000, at most 200000).")]
    pub max_chars: Option<u32>,
    #[schemars(
        description = "screenshot: low (960 px, q60), medium (default, 1280 px, q80), high (1600 px, q88) or max (native, up to 8000 px, q95)."
    )]
    pub preset: Option<Quality>,
    #[schemars(description = "screenshot: JPEG quality 1-100, overriding the preset. Recordings use video_quality.")]
    pub quality: Option<u8>,
    #[schemars(description = "record: low, medium, high (default) or max; bitrate for mp4, palette effort for gif.")]
    pub video_quality: Option<Quality>,
    #[schemars(description = "Maximum output width. screenshot: 64-2576. record: 160-3840 (default 1920, gif 960).")]
    pub max_width: Option<u32>,
    #[schemars(description = "screenshot: jpeg (default) or png. record: mp4 (default) or gif.")]
    pub format: Option<MediaFormat>,
    #[schemars(description = "screenshot: draw element ref labels on interactive elements.")]
    pub marks: Option<bool>,
    #[schemars(description = "record: start, stop (saves the file) or status.")]
    pub action: Option<RecordAction>,
    #[schemars(description = "record: frames per second, 1-60 for mp4 and 1-20 for gif (default 30, gif 20).")]
    pub fps: Option<u32>,
    #[schemars(description = "record: stop and save after this many seconds, 1-1800 (default 300).")]
    pub max_seconds: Option<u32>,
    #[schemars(description = "record: show the mouse pointer (default true).")]
    pub cursor: Option<bool>,
    #[schemars(
        description = "record: absolute .mp4 or .gif path that does not exist yet. Default: Videos\\Deckhand\\deckhand-<time>.mp4."
    )]
    pub path: Option<String>,
}

impl ObserveMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Elements => "elements",
            Self::Text => "text",
            Self::Screenshot => "screenshot",
            Self::Record => "record",
            Self::Windows => "windows",
            Self::Monitors => "monitors",
            Self::Apps => "apps",
            Self::Clipboard => "clipboard",
            Self::Status => "status",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Elements => "observe elements",
            Self::Text => "observe text",
            Self::Screenshot => "observe screenshot",
            Self::Record => "observe record",
            Self::Windows => "observe windows",
            Self::Monitors => "observe monitors",
            Self::Apps => "observe apps",
            Self::Clipboard => "observe clipboard",
            Self::Status => "observe status",
        }
    }
}

impl MediaFormat {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }

    fn image(self) -> Result<ImageFormat> {
        match self {
            Self::Jpeg => Ok(ImageFormat::Jpeg),
            Self::Png => Ok(ImageFormat::Png),
            Self::Mp4 | Self::Gif => {
                Err(Error::invalid(format!("format {} is for mode=record; screenshots take jpeg or png", self.name())))
            }
        }
    }

    fn video(self) -> Result<VideoFormat> {
        match self {
            Self::Mp4 => Ok(VideoFormat::Mp4),
            Self::Gif => Ok(VideoFormat::Gif),
            Self::Jpeg | Self::Png => Err(Error::invalid(format!(
                "format {} is for mode=screenshot; recordings take mp4 or gif",
                self.name()
            ))),
        }
    }
}

impl ObserveArgs {
    pub fn request(&self) -> Result<ObserveRequest> {
        check_fields(self.mode, &self.given_fields())?;
        match self.mode {
            ObserveMode::Elements => self.elements(),
            ObserveMode::Text => self.text().map(ObserveRequest::Text),
            ObserveMode::Screenshot => self.screenshot().map(ObserveRequest::Screenshot),
            ObserveMode::Record => self.record().map(ObserveRequest::Record),
            ObserveMode::Windows => Ok(ObserveRequest::Windows),
            ObserveMode::Monitors => Ok(ObserveRequest::Monitors),
            ObserveMode::Apps => Ok(ObserveRequest::Apps(self.apps())),
            ObserveMode::Clipboard => {
                Ok(ObserveRequest::Clipboard(ClipboardRequest { max_chars: char_limit(self.max_chars) }))
            }
            ObserveMode::Status => Ok(ObserveRequest::Status),
        }
    }

    pub fn view(&self) -> ObserveFormat {
        ObserveFormat {
            all: self.all.unwrap_or(false),
            coords: self.coords.unwrap_or(false),
            max: self.max.map_or(DEFAULT_ELEMENTS, |max| max.max(1) as usize),
        }
    }

    fn elements(&self) -> Result<ObserveRequest> {
        if self.all_windows != Some(true) {
            return Ok(ObserveRequest::Elements(ElementsRequest {
                window: self.window.clone(),
                query: non_blank(self.query.as_ref()),
            }));
        }
        self.search().map(ObserveRequest::Search)
    }

    fn search(&self) -> Result<SearchRequest> {
        let query = non_blank(self.query.as_ref()).ok_or_else(|| Error::invalid("all_windows needs query"))?;
        if self.window.is_some() {
            return Err(Error::invalid("all_windows searches every window; drop window"));
        }
        if self.all.is_some() || self.coords.is_some() {
            return Err(Error::invalid("all_windows returns match lines; drop all and coords"));
        }
        Ok(SearchRequest { query, max: self.max.map_or(DEFAULT_MATCHES, |max| max.max(1) as usize) })
    }

    fn text(&self) -> Result<TextRequest> {
        only_one(&[
            ("window", self.window.is_some()),
            ("target", self.target.is_some()),
            ("region", self.region.is_some()),
        ])?;
        let request = TextRequest {
            window: self.window.clone(),
            target: self.target.clone(),
            region: self.region,
            ocr: self.ocr.unwrap_or_default(),
            max_chars: self.max_chars,
        };
        request.validate()?;
        Ok(request)
    }

    fn screenshot(&self) -> Result<ScreenshotRequest> {
        only_one(&[
            ("target", self.target.is_some()),
            ("region", self.region.is_some()),
            ("monitor", self.monitor.is_some()),
        ])?;
        let request = ScreenshotRequest {
            target: self.target.clone(),
            region: self.region,
            monitor: self.monitor,
            preset: self.preset,
            max_width: self.max_width,
            quality: self.quality,
            format: self.format.map(MediaFormat::image).transpose()?.unwrap_or_default(),
            marks: self.marks.unwrap_or(false),
        };
        request.validate()?;
        Ok(request)
    }

    fn record(&self) -> Result<RecordRequest> {
        let action = self.action.ok_or_else(|| Error::invalid("mode=record needs action: start, stop or status"))?;
        let request = RecordRequest {
            action,
            target: self.target.clone(),
            region: self.region,
            monitor: self.monitor,
            format: self.format.map(MediaFormat::video).transpose()?,
            fps: self.fps,
            quality: self.video_quality,
            max_seconds: self.max_seconds,
            cursor: self.cursor,
            max_width: self.max_width,
            path: self.path.clone(),
        };
        request.validate()?;
        Ok(request)
    }

    fn apps(&self) -> AppsRequest {
        AppsRequest {
            query: non_blank(self.query.as_ref()),
            max: self.max.map_or(DEFAULT_APPS, |max| max.max(1) as usize),
        }
    }
}

fn only_one(sources: &[(&str, bool)]) -> Result<()> {
    let given: Vec<&str> = sources.iter().filter(|(_, given)| *given).map(|(name, _)| *name).collect();
    if given.len() <= 1 {
        return Ok(());
    }
    let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
    Err(Error::invalid(format!("give only one of {}; got {}", names.join(", "), given.join(" and "))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn args(json: &str) -> ObserveArgs {
        serde_json::from_str(json).unwrap()
    }

    fn request(json: &str) -> ObserveRequest {
        args(json).request().unwrap()
    }

    fn invalid(json: &str) -> String {
        let error = args(json).request().unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{json}");
        error.message
    }

    #[test]
    fn an_empty_call_observes_the_default_window() {
        assert_eq!(request("{}"), ObserveRequest::Elements(ElementsRequest { window: None, query: None }));
        assert_eq!(args("{}").view(), ObserveFormat::default());
    }

    #[test]
    fn element_options_shape_the_view() {
        let json = r#"{"window":"w5","query":"role:button","all":true,"coords":true,"max":40}"#;
        let ObserveRequest::Elements(found) = request(json) else { panic!("expected elements") };
        assert_eq!((found.window.as_deref(), found.query.as_deref()), (Some("w5"), Some("role:button")));
        assert_eq!(args(json).view(), ObserveFormat { all: true, coords: true, max: 40 });
    }

    #[test]
    fn a_blank_query_means_no_filter_and_max_zero_lists_one() {
        let ObserveRequest::Elements(found) = request(r#"{"query":"  "}"#) else { panic!("expected elements") };
        assert_eq!(found.query, None);
        assert_eq!(args(r#"{"max":0}"#).view().max, 1);
    }

    #[test]
    fn all_windows_searches_with_a_query() {
        assert_eq!(
            request(r#"{"query":"name:Save","all_windows":true}"#),
            ObserveRequest::Search(SearchRequest { query: "name:Save".to_string(), max: DEFAULT_MATCHES })
        );
        assert_eq!(invalid(r#"{"all_windows":true}"#), "all_windows needs query");
        assert_eq!(invalid(r#"{"all_windows":true,"query":"  "}"#), "all_windows needs query");
        assert_eq!(
            invalid(r#"{"all_windows":true,"query":"x","window":"w1"}"#),
            "all_windows searches every window; drop window"
        );
        assert!(invalid(r#"{"all_windows":true,"query":"x","coords":true}"#).contains("drop all and coords"));
        assert!(matches!(request(r#"{"all_windows":false,"query":"x"}"#), ObserveRequest::Elements(_)));
    }

    #[test]
    fn fields_of_another_mode_are_refused_by_name() {
        assert_eq!(
            invalid(r#"{"mode":"screenshot","fps":10}"#),
            "fps does not apply to mode=screenshot; it is for mode=record"
        );
        assert_eq!(
            invalid(r#"{"mode":"windows","window":"w1"}"#),
            "window does not apply to mode=windows; it is for mode=elements or text"
        );
        assert_eq!(
            invalid(r#"{"query":"x","marks":false}"#),
            "marks does not apply to mode=elements; it is for mode=screenshot"
        );
        assert!(invalid(r#"{"mode":"status","max":3}"#).starts_with("max does not apply to mode=status"));
    }

    #[test]
    fn text_reads_one_source() {
        assert_eq!(
            request(r#"{"mode":"text","target":"e4","ocr":"never","max_chars":50}"#),
            ObserveRequest::Text(TextRequest {
                window: None,
                target: Some("e4".to_string()),
                region: None,
                ocr: OcrMode::Never,
                max_chars: Some(50)
            })
        );
        assert_eq!(
            invalid(r#"{"mode":"text","window":"w1","target":"e4"}"#),
            "give only one of window, target, region; got window and target"
        );
        assert!(invalid(r#"{"mode":"text","region":{"x":0,"y":0,"width":0,"height":5}}"#).contains("positive size"));
    }

    #[test]
    fn screenshots_take_an_image_format_and_a_numeric_quality() {
        let ObserveRequest::Screenshot(shot) =
            request(r#"{"mode":"screenshot","monitor":1,"preset":"high","quality":70,"format":"png","marks":true}"#)
        else {
            panic!("expected a screenshot")
        };
        assert_eq!((shot.monitor, shot.preset, shot.quality), (Some(1), Some(Quality::High), Some(70)));
        assert_eq!((shot.format, shot.marks), (ImageFormat::Png, true));
        assert_eq!(
            invalid(r#"{"mode":"screenshot","format":"gif"}"#),
            "format gif is for mode=record; screenshots take jpeg or png"
        );
        assert_eq!(invalid(r#"{"mode":"screenshot","quality":0}"#), "quality must be from 1 to 100, got 0");
        assert!(invalid(r#"{"mode":"screenshot","target":"w1","monitor":0}"#).starts_with("give only one of target"));
        assert!(serde_json::from_str::<ObserveArgs>(r#"{"mode":"screenshot","quality":"high"}"#).is_err());
    }

    #[test]
    fn recordings_take_an_action_a_video_format_and_a_video_quality() {
        let ObserveRequest::Record(record) =
            request(r#"{"mode":"record","action":"start","format":"gif","video_quality":"max","fps":10}"#)
        else {
            panic!("expected a recording")
        };
        assert_eq!((record.action, record.format), (RecordAction::Start, Some(VideoFormat::Gif)));
        assert_eq!((record.quality, record.fps), (Some(Quality::Max), Some(10)));
        assert_eq!(invalid(r#"{"mode":"record"}"#), "mode=record needs action: start, stop or status");
        assert_eq!(
            invalid(r#"{"mode":"record","action":"start","format":"png"}"#),
            "format png is for mode=screenshot; recordings take mp4 or gif"
        );
        assert_eq!(
            invalid(r#"{"mode":"record","action":"stop","video_quality":"low"}"#),
            "mode=record action=stop takes no other fields; remove video_quality"
        );
        assert_eq!(
            invalid(r#"{"mode":"record","action":"start","quality":50}"#),
            "quality does not apply to mode=record; it is for mode=screenshot"
        );
    }

    #[test]
    fn apps_filter_and_cap() {
        assert_eq!(
            request(r#"{"mode":"apps","query":" note ","max":5}"#),
            ObserveRequest::Apps(AppsRequest { query: Some("note".to_string()), max: 5 })
        );
        assert_eq!(request(r#"{"mode":"apps"}"#), ObserveRequest::Apps(AppsRequest { query: None, max: DEFAULT_APPS }));
    }

    #[test]
    fn plain_modes_take_no_fields() {
        for (json, expected) in [
            (r#"{"mode":"windows"}"#, ObserveRequest::Windows),
            (r#"{"mode":"monitors"}"#, ObserveRequest::Monitors),
            (r#"{"mode":"clipboard"}"#, ObserveRequest::Clipboard(ClipboardRequest { max_chars: 20_000 })),
            (r#"{"mode":"status"}"#, ObserveRequest::Status),
        ] {
            assert_eq!(request(json), expected, "{json}");
        }
    }

    #[test]
    fn clipboard_text_is_capped_like_text_mode() {
        let capped = request(r#"{"mode":"clipboard","max_chars":0}"#);
        assert_eq!(capped, ObserveRequest::Clipboard(ClipboardRequest { max_chars: 1 }));
        assert_eq!(
            request(r#"{"mode":"clipboard","max_chars":500}"#),
            ObserveRequest::Clipboard(ClipboardRequest { max_chars: 500 })
        );
    }

    #[test]
    fn unknown_fields_and_modes_are_rejected() {
        assert!(serde_json::from_str::<ObserveArgs>(r#"{"windw":"x"}"#).is_err());
        assert!(serde_json::from_str::<ObserveArgs>(r#"{"mode":"find"}"#).is_err());
    }

    #[test]
    fn labels_name_the_tool_and_mode() {
        assert_eq!(ObserveMode::Screenshot.label(), "observe screenshot");
        for mode in [ObserveMode::Elements, ObserveMode::Apps, ObserveMode::Status] {
            assert_eq!(serde_json::to_value(mode).unwrap(), serde_json::json!(mode.name()));
            assert_eq!(mode.label(), format!("observe {}", mode.name()));
        }
    }
}
