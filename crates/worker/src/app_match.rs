use winpilot_core::{Error, ErrorCode, Result};
use winpilot_desktop::apps::AppEntry;

const MAX_LISTED: usize = 8;
const MAX_SUGGESTED: usize = 5;
const LAUNCH_EXTENSIONS: [&str; 3] = [".exe", ".lnk", ".bat"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Exact,
    Prefix,
    Contains,
}

const LEVELS: [Level; 3] = [Level::Exact, Level::Prefix, Level::Contains];

pub(crate) fn find<'a>(apps: &'a [AppEntry], wanted: &str) -> Result<&'a AppEntry> {
    let folded = wanted.trim().to_lowercase();
    for level in LEVELS {
        let found: Vec<&AppEntry> = apps.iter().filter(|app| matches(app, &folded, level)).collect();
        if let [only] = found.as_slice() {
            return Ok(only);
        }
        if !found.is_empty() {
            return Err(ambiguous(wanted.trim(), &found));
        }
    }
    Err(not_found(apps, wanted.trim()))
}

pub(crate) fn without_launch_extension(app: &str) -> &str {
    let lower = app.to_ascii_lowercase();
    LAUNCH_EXTENSIONS
        .iter()
        .find(|extension| lower.ends_with(*extension) && app.len() > extension.len())
        .map_or(app, |extension| &app[..app.len() - extension.len()])
}

pub(crate) fn ranked<'a>(apps: &'a [AppEntry], wanted: &str) -> Vec<&'a AppEntry> {
    let folded = wanted.trim().to_lowercase();
    let mut found: Vec<(Level, &AppEntry)> =
        apps.iter().filter_map(|app| best_level(app, &folded).map(|level| (level, app))).collect();
    found.sort_by_key(|(level, _)| *level);
    found.into_iter().map(|(_, app)| app).collect()
}

fn best_level(app: &AppEntry, wanted: &str) -> Option<Level> {
    LEVELS.into_iter().find(|&level| matches(app, wanted, level))
}

fn matches(app: &AppEntry, wanted: &str, level: Level) -> bool {
    let name = app.name.to_lowercase();
    match level {
        Level::Exact => name == wanted || app.id.to_lowercase() == wanted,
        Level::Prefix => name.starts_with(wanted),
        Level::Contains => name.contains(wanted),
    }
}

fn ambiguous(wanted: &str, found: &[&AppEntry]) -> Error {
    let mut names: Vec<String> = found.iter().take(MAX_LISTED).map(|app| label(app, found)).collect();
    if found.len() > MAX_LISTED {
        names.push("…".to_string());
    }
    Error::new(
        ErrorCode::Ambiguous,
        format!("\"{wanted}\" matches {} apps: {}; pass the full name or the app id", found.len(), names.join(", ")),
    )
}

fn label(app: &AppEntry, found: &[&AppEntry]) -> String {
    let name = app.name.to_lowercase();
    let twins = found.iter().filter(|other| other.name.to_lowercase() == name).count();
    if twins > 1 { format!("{} ({})", app.name, app.id) } else { app.name.clone() }
}

fn not_found(apps: &[AppEntry], wanted: &str) -> Error {
    let closest = closest(apps, wanted);
    if closest.is_empty() {
        return Error::not_found(format!("no installed app matches \"{wanted}\""));
    }
    Error::not_found(format!("no installed app matches \"{wanted}\"; closest: {}", closest.join(", ")))
}

fn closest<'a>(apps: &'a [AppEntry], wanted: &str) -> Vec<&'a str> {
    let wanted = wanted.to_lowercase();
    let mut scored: Vec<(usize, &str)> = apps
        .iter()
        .map(|app| (similarity(&app.name.to_lowercase(), &wanted), app.name.as_str()))
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(MAX_SUGGESTED).map(|(_, name)| name).collect()
}

fn similarity(name: &str, wanted: &str) -> usize {
    let prefix = name.chars().zip(wanted.chars()).take_while(|(a, b)| a == b).count();
    let words: usize = wanted
        .split_whitespace()
        .filter(|word| word.chars().count() > 1 && name.contains(word))
        .map(|word| word.chars().count())
        .sum();
    let pairs = bigrams(wanted).iter().filter(|pair| name.contains(pair.as_str())).count();
    prefix * 3 + words * 2 + pairs
}

fn bigrams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    chars.windows(2).map(|pair| pair.iter().collect()).collect()
}

#[cfg(test)]
mod tests {
    use winpilot_desktop::apps::AppKind;

    use super::*;

    fn app(name: &str, id: &str) -> AppEntry {
        let kind = if id.contains('!') { AppKind::Packaged } else { AppKind::Desktop };
        AppEntry { name: name.to_string(), id: id.to_string(), kind }
    }

    fn catalog() -> Vec<AppEntry> {
        vec![
            app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"),
            app("Notepad++", "{6D809377}\\Notepad++\\notepad++.exe"),
            app("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App"),
            app("Visual Studio Code", "Microsoft.VisualStudioCode"),
            app("Microsoft Teams", "MSTeams_8wekyb3d8bbwe!MSTeams"),
            app("Windows Terminal", "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
        ]
    }

    fn found(wanted: &str) -> String {
        find(&catalog(), wanted).unwrap().name.clone()
    }

    #[test]
    fn exact_name_beats_prefix_matches() {
        assert_eq!(found("notepad"), "Notepad");
        assert_eq!(found("  NOTEPAD++ "), "Notepad++");
    }

    #[test]
    fn exact_app_id_matches() {
        assert_eq!(found("microsoft.paint_8wekyb3d8bbwe!app"), "Paint");
    }

    #[test]
    fn unique_prefix_then_unique_contains() {
        assert_eq!(found("visual"), "Visual Studio Code");
        assert_eq!(found("terminal"), "Windows Terminal");
    }

    #[test]
    fn several_matches_at_the_best_level_are_ambiguous() {
        let error = find(&catalog(), "note").unwrap_err();
        assert_eq!(error.code, ErrorCode::Ambiguous);
        assert!(error.message.contains("Notepad, Notepad++"), "{}", error.message);
        let contains = find(&catalog(), "pad").unwrap_err();
        assert_eq!(contains.code, ErrorCode::Ambiguous);
    }

    #[test]
    fn identical_names_are_told_apart_by_id() {
        let apps = vec![app("Python", "python-a"), app("Python", "python-b")];
        let error = find(&apps, "python").unwrap_err();
        assert!(error.message.contains("Python (python-a), Python (python-b)"), "{}", error.message);
    }

    #[test]
    fn long_ambiguity_lists_are_capped() {
        let apps: Vec<AppEntry> =
            (0..12).map(|index| app(&format!("Tool {index}"), &format!("tool-{index}"))).collect();
        let error = find(&apps, "tool").unwrap_err();
        assert!(error.message.starts_with("\"tool\" matches 12 apps: Tool 0,"), "{}", error.message);
        assert!(error.message.contains("Tool 7, …;"), "{}", error.message);
        assert!(!error.message.contains("Tool 8"));
    }

    #[test]
    fn near_misses_suggest_close_names() {
        let error = find(&catalog(), "notpad").unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        let closest = error.message.split("closest: ").nth(1).unwrap();
        assert!(closest.starts_with("Notepad"), "{}", error.message);
        assert!(closest.split(", ").count() <= MAX_SUGGESTED);
    }

    fn ranked_names(wanted: &str) -> Vec<String> {
        ranked(&catalog(), wanted).into_iter().map(|app| app.name.clone()).collect()
    }

    #[test]
    fn ranking_puts_exact_then_prefix_then_contains() {
        let mut apps = catalog();
        apps.insert(0, app("OneNote", "Microsoft.Office.OneNote_8wekyb3d8bbwe!App"));
        let names: Vec<&str> = ranked(&apps, " NOTEPAD ").into_iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, ["Notepad", "Notepad++"]);
        let names: Vec<&str> = ranked(&apps, "note").into_iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, ["Notepad", "Notepad++", "OneNote"]);
    }

    #[test]
    fn ranking_matches_ids_exactly_and_keeps_catalog_order_within_a_level() {
        assert_eq!(ranked_names("microsoft.paint_8wekyb3d8bbwe!app"), ["Paint"]);
        assert_eq!(ranked_names("pad"), ["Notepad", "Notepad++"]);
        assert!(ranked_names("qqqq").is_empty());
    }

    #[test]
    fn launch_extensions_are_dropped_from_bare_names() {
        assert_eq!(without_launch_extension("notepad.exe"), "notepad");
        assert_eq!(without_launch_extension("Report.LNK"), "Report");
        assert_eq!(without_launch_extension("Notepad"), "Notepad");
        assert_eq!(without_launch_extension(".exe"), ".exe");
    }

    #[test]
    fn hopeless_names_suggest_nothing() {
        let error = find(&catalog(), "qqqq").unwrap_err();
        assert_eq!(error.message, "no installed app matches \"qqqq\"");
    }
}
