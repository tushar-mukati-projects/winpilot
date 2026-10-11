use base64::{Engine as _, engine::general_purpose::STANDARD};
use deckhand_capture::{EncodeOptions, Frame, FrameSource, Mark, encode};
use deckhand_core::{
    Error, ImageMapping, Rect, Result, Snapshot, Target, WindowInfo,
    format::window_line,
    protocol::{ImageFormat, ScreenshotArgs, ScreenshotReport},
    quality::{ImageQuality, screenshot_quality},
};

use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    frames,
    images::ImageRecord,
    overlay,
    read_target::{ReadTarget, read_target},
    resolve,
};

const MIN_WIDTH: u32 = 64;
const MAX_WIDTH: u32 = 2576;
const MIN_HEIGHT_LIMIT: u32 = 1568;
const MAX_NATIVE_SIDE: u32 = 8000;
const MAX_MARKS: usize = 150;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Region(Rect),
    Monitor(usize),
    Target(ReadTarget),
}

struct Shot {
    frame: Frame,
    label: String,
    window: Option<WindowInfo>,
}

pub fn run(engine: &Engine, args: &ScreenshotArgs, call: &CallScope) -> Result<ScreenshotReport> {
    args.validate()?;
    let shot = take(engine, source(args)?, call)?;
    let marks = match &shot.window {
        Some(window) if args.marks => marks(&engine.snapshot(window)?, shot.frame.rect()),
        _ => Vec::new(),
    };
    let options = encode_options(screenshot_quality(args, &engine.config.screenshot), args.format);
    report(engine, &shot, &options, &marks)
}

pub fn of_window(engine: &Engine, window: &WindowInfo) -> Result<ScreenshotReport> {
    let window = deckhand_desktop::window(window.hwnd)?;
    let shot = window_shot(engine, window)?;
    let quality = screenshot_quality(&ScreenshotArgs::default(), &engine.config.screenshot);
    report(engine, &shot, &encode_options(quality, ImageFormat::default()), &[])
}

fn source(args: &ScreenshotArgs) -> Result<Source> {
    if let Some(region) = args.region {
        return Ok(Source::Region(region));
    }
    if let Some(index) = args.monitor {
        return Ok(Source::Monitor(index as usize));
    }
    read_target(args.target.as_deref()).map(Source::Target)
}

fn take(engine: &Engine, source: Source, call: &CallScope) -> Result<Shot> {
    match source {
        Source::Region(rect) => {
            Ok(Shot { frame: frames::region(engine, rect)?, label: region_label(rect), window: None })
        }
        Source::Monitor(index) => {
            let frame = frames::monitor(engine, index)?;
            Ok(Shot { label: monitor_label(index, &frame), frame, window: None })
        }
        Source::Target(ReadTarget::DefaultWindow) => window_shot(engine, default_window(engine)?),
        Source::Target(ReadTarget::Window(spec)) => window_shot(engine, resolve::window(engine, Some(&spec))?),
        Source::Target(ReadTarget::Element(target)) => element_shot(engine, &target, call),
    }
}

fn window_shot(engine: &Engine, window: WindowInfo) -> Result<Shot> {
    let frame = frames::window(engine, &window)?;
    let label = with_source(&window_line(&window), frame.source);
    Ok(Shot { frame, label, window: Some(window) })
}

fn element_shot(engine: &Engine, target: &Target, call: &CallScope) -> Result<Shot> {
    let resolved = resolve::target(engine, target, call)?;
    let element = resolved.element().ok_or_else(|| Error::invalid("the screenshot target is not an element"))?;
    let frame = frames::element(engine, &resolved.window, element.rect)?;
    let label = with_source(&format!("{} in {}", element.id, window_line(&resolved.window)), frame.source);
    Ok(Shot { frame, label, window: Some(resolved.window) })
}

pub fn with_source(label: &str, source: FrameSource) -> String {
    let pixels = match source {
        FrameSource::Window => "window pixels",
        FrameSource::Screen => "screen pixels",
        FrameSource::ScreenFallback => "screen pixels; the window could not be printed",
    };
    format!("{label} ({pixels})")
}

fn marks(snapshot: &Snapshot, frame: Rect) -> Vec<Mark> {
    snapshot
        .elements
        .iter()
        .filter(|element| element.is_visible() && element.is_actionable())
        .filter_map(|element| Some(Mark { rect: element.rect.intersection(frame)?, label: element.id.to_string() }))
        .take(MAX_MARKS)
        .collect()
}

fn encode_options(wanted: ImageQuality, format: ImageFormat) -> EncodeOptions {
    let quality = wanted.quality.clamp(1, 100);
    let Some(max_width) = wanted.max_width else {
        return EncodeOptions { max_width: MAX_NATIVE_SIDE, max_height: MAX_NATIVE_SIDE, format, quality };
    };
    let max_width = max_width.clamp(MIN_WIDTH, MAX_WIDTH);
    EncodeOptions { max_width, max_height: max_width.max(MIN_HEIGHT_LIMIT), format, quality }
}

fn report(engine: &Engine, shot: &Shot, options: &EncodeOptions, marks: &[Mark]) -> Result<ScreenshotReport> {
    let encoded = encode(&shot.frame, options, marks)?;
    let mapping = ImageMapping { origin: shot.frame.origin, scale: encoded.scale };
    let record = ImageRecord {
        mapping,
        width: encoded.width,
        height: encoded.height,
        window: shot.window.as_ref().map(|window| window.hwnd),
    };
    let image = engine.images.lock().insert(record);
    overlay::screenshot(engine, shot.frame.rect());
    Ok(ScreenshotReport {
        image,
        mime: encoded.mime.to_string(),
        data: STANDARD.encode(&encoded.bytes),
        width: encoded.width,
        height: encoded.height,
        mapping,
        source: shot.label.clone(),
        quality: (options.format == ImageFormat::Jpeg).then_some(options.quality),
    })
}

fn region_label(rect: Rect) -> String {
    format!("region {},{} {}x{}", rect.x, rect.y, rect.width, rect.height)
}

fn monitor_label(index: usize, frame: &Frame) -> String {
    format!("monitor {index} {}x{}", frame.width, frame.height)
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementRef, ErrorCode, Point, Role, refs::looks_like_window_ref};

    use super::*;
    use crate::test_fixture::{FixtureApp, element, engine, snapshot};

    const JPEG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];
    const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];

    fn args() -> ScreenshotArgs {
        ScreenshotArgs::default()
    }

    fn shoot(args: &ScreenshotArgs) -> Result<ScreenshotReport> {
        run(engine(), args, &CallScope::relaxed())
    }

    fn bytes(report: &ScreenshotReport) -> Vec<u8> {
        STANDARD.decode(&report.data).unwrap()
    }

    fn assert_round_trips(report: &ScreenshotReport) {
        let corner = Point::new(report.width as i32 - 1, report.height as i32 - 1);
        assert_eq!(report.mapping.to_image(report.mapping.to_screen(corner)), corner);
        assert_eq!(report.mapping.to_screen(Point::new(0, 0)), report.mapping.origin);
        let record = engine().images.lock().record(report.image).unwrap();
        assert_eq!((record.mapping, record.width, record.height), (report.mapping, report.width, report.height));
    }

    fn assert_covers(report: &ScreenshotReport, rect: Rect) {
        assert_eq!(report.mapping.origin, Point::new(rect.x, rect.y));
        let scale = report.mapping.scale;
        assert!((f64::from(report.width) * scale - f64::from(rect.width)).abs() <= scale.max(1.0));
        assert!((f64::from(report.height) * scale - f64::from(rect.height)).abs() <= scale.max(1.0));
    }

    #[test]
    fn region_wins_over_monitor_and_target() {
        let region = Rect::new(1, 2, 3, 4);
        let all = ScreenshotArgs { region: Some(region), monitor: Some(1), target: Some("e1".to_string()), ..args() };
        assert_eq!(source(&all).unwrap(), Source::Region(region));
        let monitor = ScreenshotArgs { monitor: Some(1), target: Some("e1".to_string()), ..args() };
        assert_eq!(source(&monitor).unwrap(), Source::Monitor(1));
    }

    #[test]
    fn target_or_default_comes_last() {
        let element = ScreenshotArgs { target: Some("e7".to_string()), ..args() };
        assert_eq!(source(&element).unwrap(), Source::Target(ReadTarget::Element(Target::Element(ElementRef(7)))));
        let window = ScreenshotArgs { target: Some("process:notepad".to_string()), ..args() };
        assert_eq!(source(&window).unwrap(), Source::Target(ReadTarget::Window("process:notepad".to_string())));
        assert_eq!(source(&args()).unwrap(), Source::Target(ReadTarget::DefaultWindow));
    }

    #[test]
    fn empty_region_is_invalid() {
        let empty = ScreenshotArgs { region: Some(Rect::new(0, 0, 0, 10)), ..args() };
        assert_eq!(shoot(&empty).unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn labels_say_where_the_pixels_came_from() {
        assert_eq!(with_source("w1 a.exe", FrameSource::Window), "w1 a.exe (window pixels)");
        assert_eq!(with_source("w1 a.exe", FrameSource::Screen), "w1 a.exe (screen pixels)");
        assert_eq!(
            with_source("w1 a.exe", FrameSource::ScreenFallback),
            "w1 a.exe (screen pixels; the window could not be printed)"
        );
    }

    #[test]
    fn encode_options_clamp_the_width_and_keep_a_tall_limit() {
        let config = deckhand_core::Config::default();
        let default = encode_options(screenshot_quality(&args(), &config.screenshot), ImageFormat::Jpeg);
        assert_eq!((default.max_width, default.max_height, default.quality), (1280, 1568, 80));
        let narrow = ImageQuality { max_width: Some(10), quality: 80 };
        assert_eq!(encode_options(narrow, ImageFormat::Png).max_width, 64);
        let wide = encode_options(ImageQuality { max_width: Some(9000), quality: 0 }, ImageFormat::Png);
        assert_eq!((wide.max_width, wide.max_height, wide.format, wide.quality), (2576, 2576, ImageFormat::Png, 1));
    }

    #[test]
    fn the_max_preset_keeps_native_resolution_up_to_8000_px_per_side() {
        let config = deckhand_core::Config::default();
        let max = ScreenshotArgs { preset: Some(deckhand_core::protocol::Quality::Max), ..args() };
        let options = encode_options(screenshot_quality(&max, &config.screenshot), ImageFormat::Jpeg);
        assert_eq!((options.max_width, options.max_height, options.quality), (8000, 8000, 95));
    }

    #[test]
    fn marks_cover_visible_actionable_elements_inside_the_frame() {
        let mut hidden = element(3, 1, Role::Button, "Hidden");
        hidden.state.offscreen = true;
        let mut outside = element(4, 1, Role::Button, "Outside");
        outside.rect = Rect::new(500, 500, 10, 10);
        let mut straddling = element(5, 1, Role::Button, "Straddling");
        straddling.rect = Rect::new(90, 90, 20, 20);
        let elements =
            vec![element(1, 0, Role::Button, "OK"), element(2, 1, Role::Text, "Label"), hidden, outside, straddling];
        let found = marks(&snapshot("Dialog", elements), Rect::new(0, 0, 100, 100));
        let labels: Vec<&str> = found.iter().map(|mark| mark.label.as_str()).collect();
        assert_eq!(labels, ["e1", "e5"]);
        assert_eq!(found[1].rect, Rect::new(90, 90, 10, 10));
    }

    #[test]
    fn marks_are_capped() {
        let elements = (1..=200).map(|id| element(id, 1, Role::Button, "B")).collect();
        assert_eq!(marks(&snapshot("Many", elements), Rect::new(0, 0, 100, 100)).len(), MAX_MARKS);
    }

    #[test]
    fn region_and_monitor_labels() {
        assert_eq!(region_label(Rect::new(0, 0, 800, 600)), "region 0,0 800x600");
        let frame = Frame {
            origin: Point::new(0, 0),
            width: 2560,
            height: 1600,
            bgra: Vec::new(),
            source: FrameSource::Screen,
        };
        assert_eq!(monitor_label(0, &frame), "monitor 0 2560x1600");
    }

    #[test]
    fn primary_monitor_round_trips() {
        let primary = engine().capturer.monitor_rects().unwrap()[0];
        let report = shoot(&ScreenshotArgs { monitor: Some(0), ..args() }).unwrap();
        assert_covers(&report, primary);
        assert!(report.width <= 1280 && report.height <= 1568);
        assert_eq!(report.source, format!("monitor 0 {}x{}", primary.width, primary.height));
        assert_eq!((report.mime.as_str(), report.quality), ("image/jpeg", Some(80)));
        assert!(bytes(&report).starts_with(&JPEG_MAGIC));
        assert_round_trips(&report);
        assert_eq!(engine().images.lock().record(report.image).unwrap().window, None);
    }

    #[test]
    fn small_region_is_unscaled_png() {
        let primary = engine().capturer.monitor_rects().unwrap()[0];
        let region = Rect::new(primary.x + 10, primary.y + 10, 200, 100);
        let wanted = ScreenshotArgs { region: Some(region), format: ImageFormat::Png, ..args() };
        let report = shoot(&wanted).unwrap();
        assert_eq!((report.width, report.height, report.mapping.scale, report.quality), (200, 100, 1.0, None));
        assert_eq!(report.source, format!("region {},{} 200x100", region.x, region.y));
        assert!(bytes(&report).starts_with(&PNG_MAGIC));
        assert_round_trips(&report);
    }

    #[test]
    fn presets_change_the_size_and_quality_of_a_monitor_shot() {
        let primary = engine().capturer.monitor_rects().unwrap()[0];
        let preset = |quality| ScreenshotArgs { monitor: Some(0), preset: Some(quality), ..args() };
        let low = shoot(&preset(deckhand_core::protocol::Quality::Low)).unwrap();
        assert!(low.width <= 960 && low.quality == Some(60), "{}x{} {:?}", low.width, low.height, low.quality);
        let max = shoot(&preset(deckhand_core::protocol::Quality::Max)).unwrap();
        assert_eq!((max.width, max.height, max.quality), (primary.width as u32, primary.height as u32, Some(95)));
        assert_round_trips(&max);
        let overridden =
            ScreenshotArgs { max_width: Some(640), quality: Some(30), ..preset(deckhand_core::protocol::Quality::Max) };
        let small = shoot(&overridden).unwrap();
        assert!(small.width <= 640 && small.quality == Some(30));
    }

    #[test]
    fn out_of_range_quality_is_invalid() {
        let error = shoot(&ScreenshotArgs { quality: Some(0), ..args() }).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn bad_monitor_index_is_invalid() {
        let error = shoot(&ScreenshotArgs { monitor: Some(99), ..args() }).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn default_window_round_trips() {
        if default_window(engine()).is_err() {
            return;
        }
        let report = match shoot(&args()) {
            Err(error) if matches!(error.code, ErrorCode::NotFound | ErrorCode::InvalidArgument) => return,
            result => result.unwrap(),
        };
        assert!(report.width <= 1280 && report.height <= 1568);
        assert!(report.source.split_whitespace().next().is_some_and(looks_like_window_ref));
        assert!(report.source.ends_with("pixels)") || report.source.ends_with("printed)"), "{}", report.source);
        assert_round_trips(&report);
        assert!(engine().images.lock().record(report.image).unwrap().window.is_some());
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn background_fixture_window_is_captured_with_marks() {
        let Some(app) = FixtureApp::launch() else { return };
        let target = Some(app.hwnd().to_string());
        let report = shoot(&ScreenshotArgs { target, marks: true, ..args() }).unwrap();
        assert_covers(&report, app.window.rect);
        assert!(report.source.starts_with(&app.hwnd().to_string()));
        assert!(report.source.contains(app.title()));
        assert_round_trips(&report);
        assert_eq!(engine().images.lock().record(report.image).unwrap().window, Some(app.hwnd()));
        let plain = of_window(engine(), &app.window).unwrap();
        assert_covers(&plain, app.window.rect);
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_element_is_cropped() {
        let Some(app) = FixtureApp::launch() else { return };
        let tree = engine().snapshot(&app.window).unwrap();
        let button = tree.elements.iter().find(|found| found.automation_id == "2003").unwrap().clone();
        let report = shoot(&ScreenshotArgs { target: Some(button.id.to_string()), ..args() }).unwrap();
        assert_covers(&report, button.rect.intersection(app.window.rect).unwrap());
        assert!(report.source.starts_with(&format!("{} in {}", button.id, app.hwnd())));
        let again = shoot(&ScreenshotArgs { target: Some(app.selector("id:2003")), ..args() }).unwrap();
        assert_eq!(again.mapping.origin, report.mapping.origin);
    }
}
