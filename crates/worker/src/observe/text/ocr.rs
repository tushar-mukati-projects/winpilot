use winpilot_capture::{OcrPass, ocr};
use winpilot_core::{
    Result,
    ocr::{OcrLine, line_with_coords},
    protocol::TextSource,
};

use super::Reading;
use crate::frames::Captured;

pub(super) fn ocr_reading(captured: &Captured, joined: fn(&[OcrLine]) -> String) -> Result<Reading> {
    let lines = ocr(&captured.frame, OcrPass::Plain)?;
    Ok(Reading { text: joined(&lines), source: TextSource::Ocr, note: captured.note })
}

pub(super) fn plain_text(lines: &[OcrLine]) -> String {
    lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
}

pub(super) fn located_text(lines: &[OcrLine]) -> String {
    lines.iter().map(line_with_coords).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;

    #[test]
    fn located_lines_end_with_their_screen_rectangles() {
        let lines = [
            OcrLine { text: "Import".to_string(), rect: Rect::new(10, 20, 60, 16), words: Vec::new() },
            OcrLine { text: "Export video".to_string(), rect: Rect::new(-1900, 40, 120, 16), words: Vec::new() },
        ];
        assert_eq!(located_text(&lines), "Import @10,20,60x16\nExport video @-1900,40,120x16");
        assert_eq!(located_text(&[]), "");
    }
}
