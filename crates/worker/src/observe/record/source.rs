use winpilot_capture::RecordingSource;
use winpilot_core::{
    Error, Rect, Result, Target, WindowInfo, WindowRef, WindowState, format::other_desktop, protocol::RecordRequest,
};

use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    read_target::{ReadTarget, read_target},
    resolve,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Redaction {
    DeniedWindows,
    WholeWindowWhileDenied(WindowRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) capture: RecordingSource,
    pub(crate) area: Rect,
    pub(crate) redaction: Redaction,
}

pub(crate) fn resolve(engine: &Engine, args: &RecordRequest, call: &CallScope) -> Result<Source> {
    if let Some(region) = args.region {
        return Ok(screen(region));
    }
    if let Some(index) = args.monitor {
        return monitor(&engine.capturer.monitor_rects()?, index);
    }
    match read_target(args.target.as_deref())? {
        ReadTarget::DefaultWindow => window(engine, &default_window(engine)?, None),
        ReadTarget::Window(spec) => window(engine, &resolve::window(engine, Some(&spec))?, None),
        ReadTarget::Element(target) => element(engine, &target, call),
    }
}

const fn screen(rect: Rect) -> Source {
    Source { capture: RecordingSource::Screen(rect), area: rect, redaction: Redaction::DeniedWindows }
}

fn monitor(rects: &[Rect], index: u32) -> Result<Source> {
    let rect = rects.get(index as usize).copied().ok_or_else(|| {
        Error::invalid(format!("monitor index {index} is out of range; {} monitor(s) are attached", rects.len()))
    })?;
    Ok(screen(rect))
}

fn window(engine: &Engine, window: &WindowInfo, crop: Option<Rect>) -> Result<Source> {
    engine.guard.check_window(window)?;
    window_source(window, crop)
}

fn window_source(window: &WindowInfo, crop: Option<Rect>) -> Result<Source> {
    if window.state == WindowState::Minimized {
        return Err(Error::invalid(format!(
            "{} is minimized; restore it first (act window action=restore)",
            window.hwnd
        )));
    }
    if let Some(desktop) = other_desktop(window) {
        return Err(Error::unsupported(format!(
            "{} is on {desktop}; Windows doesn't draw it there, so it can't be recorded; move it to this desktop",
            window.hwnd
        )));
    }
    let area = crop
        .map_or(window.rect, |crop| Rect::new(window.rect.x + crop.x, window.rect.y + crop.y, crop.width, crop.height));
    Ok(Source {
        capture: RecordingSource::Window { window: window.hwnd, crop },
        area,
        redaction: Redaction::WholeWindowWhileDenied(window.hwnd),
    })
}

fn element(engine: &Engine, target: &Target, call: &CallScope) -> Result<Source> {
    let resolved = resolve::target(engine, target, call)?;
    let element =
        resolved.element_or_point_hit().ok_or_else(|| Error::invalid("the record target is not an element"))?;
    let crop = crop_within(resolved.window.rect, element.rect)?;
    window(engine, &resolved.window, Some(crop))
}

fn crop_within(window: Rect, element: Rect) -> Result<Rect> {
    let visible = element
        .intersection(window)
        .ok_or_else(|| Error::invalid("the element is outside its window's visible area"))?;
    Ok(Rect::new(visible.x - window.x, visible.y - window.y, visible.width, visible.height))
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;
    use crate::test_fixture;

    #[test]
    fn regions_and_monitors_are_redacted_screen_recordings() {
        let region = Rect::new(-100, 20, 640, 480);
        assert_eq!(
            screen(region),
            Source { capture: RecordingSource::Screen(region), area: region, redaction: Redaction::DeniedWindows }
        );
        let rects = [Rect::new(0, 0, 2560, 1600), Rect::new(-1920, 0, 1920, 1080)];
        assert_eq!(monitor(&rects, 1).unwrap().capture, RecordingSource::Screen(rects[1]));
        let missing = monitor(&rects, 2).unwrap_err();
        assert_eq!(missing.code, ErrorCode::InvalidArgument);
        assert_eq!(missing.message, "monitor index 2 is out of range; 2 monitor(s) are attached");
    }

    #[test]
    fn windows_are_watched_for_the_deny_list_and_elements_crop_relative_to_the_window() {
        let info = test_fixture::window(42, "app.exe", "App", Rect::new(100, 50, 800, 600));
        let whole = window_source(&info, None).unwrap();
        assert_eq!(whole.capture, RecordingSource::Window { window: WindowRef(42), crop: None });
        assert_eq!((whole.area, whole.redaction), (info.rect, Redaction::WholeWindowWhileDenied(WindowRef(42))));
        let crop = crop_within(info.rect, Rect::new(80, 70, 100, 40)).unwrap();
        assert_eq!(crop, Rect::new(0, 20, 80, 40));
        let cropped = window_source(&info, Some(crop)).unwrap();
        assert_eq!(cropped.area, Rect::new(100, 70, 80, 40));
        assert!(crop_within(info.rect, Rect::new(2000, 2000, 10, 10)).is_err());
    }

    #[test]
    fn minimized_windows_are_refused() {
        let info = WindowInfo {
            state: WindowState::Minimized,
            ..test_fixture::window(7, "app.exe", "App", Rect::new(0, 0, 10, 10))
        };
        let error = window_source(&info, None).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.contains("is minimized"), "{}", error.message);
    }

    #[test]
    fn windows_on_other_desktops_are_refused() {
        let elsewhere = WindowInfo {
            desktop: Some(2),
            on_current_desktop: false,
            ..test_fixture::window(8, "app.exe", "App", Rect::new(0, 0, 10, 10))
        };
        let error = window_source(&elsewhere, None).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(
            error.message,
            "w8 is on desktop 2; Windows doesn't draw it there, so it can't be recorded; move it to this desktop"
        );
    }

    #[test]
    fn denied_windows_are_refused() {
        let vault = test_fixture::window(9, "KeePass.exe", "Vault", Rect::new(0, 0, 10, 10));
        let error = window(test_fixture::engine(), &vault, None).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
    }
}
