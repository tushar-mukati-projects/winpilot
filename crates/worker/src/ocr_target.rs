use deckhand_core::{
    Error, ErrorCode, Result, WindowInfo,
    format::window_line,
    ocr::{find_matches, pick_match},
};

use crate::{
    call_scope::CallScope,
    engine::Engine,
    ocr_read::{Freshness, magnified_window_lines, window_lines},
    resolve::{Reach, Resolved},
};

#[derive(Clone, Copy)]
pub(crate) struct OcrUse<'a> {
    pub(crate) call: &'a CallScope,
    pub(crate) freshness: Freshness,
}

pub(crate) fn resolve(
    engine: &Engine,
    window: WindowInfo,
    text: &str,
    nth: Option<u32>,
    using: OcrUse<'_>,
) -> Result<Resolved> {
    engine.guard.check_window(&window)?;
    let lines = window_lines(engine, &window, Some(using.call), using.freshness)?;
    let mut matches = find_matches(&lines, text);
    if matches.is_empty() {
        matches = find_matches(&magnified_window_lines(engine, &window)?, text);
    }
    let found = pick_match(matches, text, nth).map_err(|error| in_window(error, &window))?;
    let point = found.rect.center();
    let under = engine.uia.run(move |uia| uia.element_at(point))?;
    Ok(Resolved { window, reach: Reach::Point { point, under } })
}

fn in_window(error: Error, window: &WindowInfo) -> Error {
    if error.code != ErrorCode::NotFound {
        return error;
    }
    Error::new(error.code, format!("{} in {}", error.message, window_line(window)))
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    #[test]
    fn missing_text_names_the_window_that_was_read() {
        let app = window(5, "capcut.exe", "CapCut", Rect::new(0, 0, 10, 10));
        let error = in_window(Error::not_found("no text on screen matches ocr:\"Export\""), &app);
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.message, "no text on screen matches ocr:\"Export\" in w5 capcut.exe \"CapCut\"");
        let ambiguous = in_window(Error::new(ErrorCode::Ambiguous, "2 places"), &app);
        assert_eq!(ambiguous.message, "2 places");
    }
}
