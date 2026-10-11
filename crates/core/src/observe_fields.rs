use crate::{
    error::{Error, Result},
    observe::{ObserveArgs, ObserveMode},
};

use ObserveMode::{Apps, Clipboard, Elements, Record, Screenshot, Text};

const FIELD_MODES: [(&str, &[ObserveMode]); 22] = [
    ("window", &[Elements, Text]),
    ("target", &[Text, Screenshot, Record]),
    ("query", &[Elements, Apps]),
    ("all_windows", &[Elements]),
    ("all", &[Elements]),
    ("coords", &[Elements]),
    ("max", &[Elements, Apps]),
    ("region", &[Text, Screenshot, Record]),
    ("monitor", &[Screenshot, Record]),
    ("ocr", &[Text]),
    ("max_chars", &[Text, Clipboard]),
    ("preset", &[Screenshot]),
    ("quality", &[Screenshot]),
    ("video_quality", &[Record]),
    ("max_width", &[Screenshot, Record]),
    ("format", &[Screenshot, Record]),
    ("marks", &[Screenshot]),
    ("action", &[Record]),
    ("fps", &[Record]),
    ("max_seconds", &[Record]),
    ("cursor", &[Record]),
    ("path", &[Record]),
];

impl ObserveArgs {
    pub(crate) fn given_fields(&self) -> Vec<&'static str> {
        [
            ("window", self.window.is_some()),
            ("target", self.target.is_some()),
            ("query", self.query.is_some()),
            ("all_windows", self.all_windows.is_some()),
            ("all", self.all.is_some()),
            ("coords", self.coords.is_some()),
            ("max", self.max.is_some()),
            ("region", self.region.is_some()),
            ("monitor", self.monitor.is_some()),
            ("ocr", self.ocr.is_some()),
            ("max_chars", self.max_chars.is_some()),
            ("preset", self.preset.is_some()),
            ("quality", self.quality.is_some()),
            ("video_quality", self.video_quality.is_some()),
            ("max_width", self.max_width.is_some()),
            ("format", self.format.is_some()),
            ("marks", self.marks.is_some()),
            ("action", self.action.is_some()),
            ("fps", self.fps.is_some()),
            ("max_seconds", self.max_seconds.is_some()),
            ("cursor", self.cursor.is_some()),
            ("path", self.path.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, given)| given.then_some(name))
        .collect()
    }
}

pub(crate) fn check_fields(mode: ObserveMode, given: &[&str]) -> Result<()> {
    let Some(stray) = given.iter().find(|field| !modes_of(field).contains(&mode)) else { return Ok(()) };
    Err(Error::invalid(format!(
        "{stray} does not apply to mode={}; it is for mode={}",
        mode.name(),
        mode_list(modes_of(stray))
    )))
}

fn modes_of(field: &str) -> &'static [ObserveMode] {
    FIELD_MODES.iter().find(|(name, _)| *name == field).map_or(&[], |(_, modes)| modes)
}

fn mode_list(modes: &[ObserveMode]) -> String {
    let names: Vec<&str> = modes.iter().map(|mode| mode.name()).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_given_field_has_a_home_mode() {
        let everything = ObserveArgs {
            window: Some(String::new()),
            target: Some(String::new()),
            query: Some(String::new()),
            all_windows: Some(true),
            all: Some(true),
            coords: Some(true),
            max: Some(1),
            region: Some(crate::geometry::Rect::new(0, 0, 1, 1)),
            monitor: Some(0),
            ocr: Some(crate::observe::OcrMode::Auto),
            max_chars: Some(1),
            preset: Some(crate::quality::Quality::Low),
            quality: Some(1),
            video_quality: Some(crate::quality::Quality::Low),
            max_width: Some(100),
            format: Some(crate::observe::MediaFormat::Png),
            marks: Some(true),
            action: Some(crate::recording::RecordAction::Status),
            fps: Some(1),
            max_seconds: Some(1),
            cursor: Some(true),
            path: Some(String::new()),
            mode: Elements,
        };
        let given = everything.given_fields();
        assert_eq!(given.len(), FIELD_MODES.len());
        assert!(given.iter().all(|field| !modes_of(field).is_empty()), "{given:?}");
    }

    #[test]
    fn the_status_mode_takes_nothing() {
        assert!(check_fields(ObserveMode::Status, &[]).is_ok());
        let error = check_fields(ObserveMode::Status, &["target"]).unwrap_err();
        assert_eq!(error.message, "target does not apply to mode=status; it is for mode=text, screenshot or record");
    }

    #[test]
    fn mode_lists_read_naturally() {
        assert_eq!(mode_list(&[Record]), "record");
        assert_eq!(mode_list(&[Elements, Apps]), "elements or apps");
        assert_eq!(mode_list(&[Text, Screenshot, Record]), "text, screenshot or record");
    }
}
