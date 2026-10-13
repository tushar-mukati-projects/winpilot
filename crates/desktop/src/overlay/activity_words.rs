use super::{
    activity::{Activity, Cue, OutOfSight},
    chip_text::ChipText,
    pill_words::{PillWords, SEPARATOR},
};

pub(crate) fn activity_hint(activity: &Activity) -> String {
    let place = place_words(&activity.app, activity.out_of_sight);
    match activity.cue.as_ref().and_then(cue_words) {
        Some(cue) => format!("{SEPARATOR}{place}{SEPARATOR}{cue}"),
        None => format!("{SEPARATOR}{place}"),
    }
}

pub(crate) fn activity_words(label: &str, hint: &str) -> PillWords {
    PillWords { lead: label.to_string(), hint: Some(hint.to_string()) }
}

fn place_words(app: &str, out_of_sight: OutOfSight) -> String {
    match out_of_sight {
        OutOfSight::Behind => format!("working in {app} behind your windows"),
        OutOfSight::Minimized => format!("working in {app} (minimized)"),
        OutOfSight::OnDesktop(Some(number)) => format!("working in {app} on desktop {number}"),
        OutOfSight::OnDesktop(None) => format!("working in {app} on another desktop"),
    }
}

fn cue_words(cue: &Cue) -> Option<String> {
    let words = match cue {
        Cue::Click(0 | 1) => "click".to_string(),
        Cue::Click(2) => "double-click".to_string(),
        Cue::Click(_) => "triple-click".to_string(),
        Cue::Scroll => "scroll".to_string(),
        Cue::Drag => "drag".to_string(),
        Cue::Typing => "typing".to_string(),
        Cue::Keys(chords) => ChipText::keys(chords.clone()).spoken(),
    };
    (!words.is_empty()).then_some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(out_of_sight: OutOfSight, cue: Option<Cue>) -> String {
        activity_hint(&Activity { app: "CapCut".to_string(), out_of_sight, cue })
    }

    fn keys(chords: &[&[&str]]) -> Cue {
        Cue::Keys(chords.iter().map(|chord| chord.iter().map(|key| (*key).to_string()).collect()).collect())
    }

    #[test]
    fn the_hint_says_where_the_hidden_app_is() {
        assert_eq!(hint(OutOfSight::Behind, None), " \u{00B7} working in CapCut behind your windows");
        assert_eq!(hint(OutOfSight::Minimized, None), " \u{00B7} working in CapCut (minimized)");
        assert_eq!(hint(OutOfSight::OnDesktop(Some(2)), None), " \u{00B7} working in CapCut on desktop 2");
        assert_eq!(hint(OutOfSight::OnDesktop(None), None), " \u{00B7} working in CapCut on another desktop");
    }

    #[test]
    fn the_hint_ends_with_what_was_just_done() {
        let behind = |cue| hint(OutOfSight::Behind, Some(cue));
        assert_eq!(behind(Cue::Click(1)), " \u{00B7} working in CapCut behind your windows \u{00B7} click");
        assert!(behind(Cue::Click(2)).ends_with(" \u{00B7} double-click"));
        assert!(behind(Cue::Click(3)).ends_with(" \u{00B7} triple-click"));
        assert!(behind(Cue::Scroll).ends_with(" \u{00B7} scroll"));
        assert!(behind(Cue::Drag).ends_with(" \u{00B7} drag"));
        assert!(behind(Cue::Typing).ends_with(" \u{00B7} typing"));
    }

    #[test]
    fn keys_name_the_key_and_typed_characters_stay_hidden() {
        let behind = |cue| hint(OutOfSight::Behind, Some(cue));
        assert!(behind(keys(&[&["Enter"]])).ends_with(" \u{00B7} Enter"));
        assert!(behind(keys(&[&["Tab"], &["Enter"]])).ends_with(" \u{00B7} Tab, Enter"));
        assert!(behind(keys(&[&["P"], &["W"]])).ends_with(" \u{00B7} typing"));
        assert_eq!(behind(keys(&[])), " \u{00B7} working in CapCut behind your windows");
    }

    #[test]
    fn the_pill_leads_with_the_agent() {
        let words = activity_words("Claude", " \u{00B7} working in CapCut behind your windows");
        assert_eq!(words.lead, "Claude");
        assert_eq!(words.hint.as_deref(), Some(" \u{00B7} working in CapCut behind your windows"));
    }
}
