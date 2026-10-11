use std::time::Duration;

use deckhand_core::{
    Result,
    protocol::{AppInfo, AppKind, AppsRequest, WindowsReport},
};
use deckhand_desktop::apps::{self, AppEntry};

use crate::{
    app_match::{self, without_launch_extension},
    engine::Engine,
};

const FRESH_LISTING: Duration = Duration::from_secs(300);
const RECENT_LISTING: Duration = Duration::from_secs(10);

pub fn list(engine: &Engine, request: &AppsRequest) -> Result<WindowsReport> {
    let Some(query) = request.query.as_deref() else {
        return Ok(every_app(&engine.apps.apps(FRESH_LISTING)?, request.max));
    };
    let query = without_launch_extension(query);
    let catalog = engine.apps.apps(FRESH_LISTING)?;
    if app_match::ranked(&catalog, query).is_empty() {
        return Ok(matching(&engine.apps.apps(RECENT_LISTING)?, query, request.max));
    }
    Ok(matching(&catalog, query, request.max))
}

fn every_app(catalog: &[AppEntry], max: usize) -> WindowsReport {
    let mut apps: Vec<AppInfo> = catalog.iter().map(app_info).collect();
    apps.sort_by_cached_key(|app| app.name.to_lowercase());
    let total = apps.len();
    let note = if total > max {
        format!("showing {max} of {total} installed apps; narrow with query or raise max")
    } else {
        format!("{total} installed apps")
    };
    apps.truncate(max);
    WindowsReport { apps, note: Some(note), ..WindowsReport::default() }
}

fn matching(catalog: &[AppEntry], query: &str, max: usize) -> WindowsReport {
    let found = app_match::ranked(catalog, query);
    let note = match found.len() {
        0 => Some(format!("no installed app matches \"{query}\"")),
        count if count > max => {
            Some(format!("showing {max} of {count} apps matching \"{query}\"; narrow the query or raise max"))
        }
        _ => None,
    };
    let apps = found.into_iter().take(max).map(app_info).collect();
    WindowsReport { apps, note, ..WindowsReport::default() }
}

fn app_info(app: &AppEntry) -> AppInfo {
    let kind = match app.kind {
        apps::AppKind::Packaged => AppKind::Packaged,
        apps::AppKind::Desktop => AppKind::Desktop,
    };
    AppInfo { name: app.name.clone(), id: app.id.clone(), kind }
}

#[cfg(test)]
mod tests {
    use deckhand_core::format;

    use super::*;
    use crate::test_fixture::engine;

    const MAX: usize = 30;

    fn entry(name: &str, id: &str, kind: apps::AppKind) -> AppEntry {
        AppEntry { name: name.to_string(), id: id.to_string(), kind }
    }

    fn names(report: &WindowsReport) -> Vec<&str> {
        report.apps.iter().map(|app| app.name.as_str()).collect()
    }

    fn request(query: Option<&str>, max: usize) -> AppsRequest {
        AppsRequest { query: query.map(str::to_string), max }
    }

    #[test]
    fn entries_convert_with_their_kind() {
        let packaged = app_info(&entry("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App", apps::AppKind::Packaged));
        assert_eq!(packaged.kind, AppKind::Packaged);
        assert_eq!((packaged.name.as_str(), packaged.id.as_str()), ("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App"));
        assert_eq!(app_info(&entry("Tool", "C:\\tool.exe", apps::AppKind::Desktop)).kind, AppKind::Desktop);
    }

    #[test]
    fn matches_are_ranked_and_capped() {
        let mut catalog: Vec<AppEntry> = (0..40)
            .map(|index| entry(&format!("Tool {index}"), &format!("tool-{index}"), apps::AppKind::Desktop))
            .collect();
        catalog.push(entry("Tool", "tool", apps::AppKind::Desktop));
        let report = matching(&catalog, "tool", MAX);
        assert_eq!(report.apps.len(), MAX);
        assert_eq!(report.apps[0].name, "Tool");
        assert_eq!(
            report.note.as_deref(),
            Some("showing 30 of 41 apps matching \"tool\"; narrow the query or raise max")
        );
        let few = matching(&catalog, "tool 39", MAX);
        assert_eq!(names(&few), ["Tool 39"]);
        assert_eq!(few.note, None);
        assert_eq!(matching(&catalog, "tool", 3).apps.len(), 3);
    }

    #[test]
    fn no_match_says_so() {
        let catalog = vec![entry("Paint", "paint", apps::AppKind::Desktop)];
        let report = matching(&catalog, "qqqq", MAX);
        assert!(report.apps.is_empty());
        assert_eq!(format::windows(&report), "no installed app matches \"qqqq\"");
    }

    #[test]
    fn the_full_list_is_sorted_by_name() {
        let catalog = vec![
            entry("paint", "p", apps::AppKind::Desktop),
            entry("Calculator", "c", apps::AppKind::Packaged),
            entry("Notepad", "n", apps::AppKind::Packaged),
        ];
        let report = every_app(&catalog, MAX);
        assert_eq!(names(&report), ["Calculator", "Notepad", "paint"]);
        assert_eq!(report.note.as_deref(), Some("3 installed apps"));
    }

    #[test]
    fn the_full_list_is_capped_with_a_hint_to_filter() {
        let catalog: Vec<AppEntry> = (0..200)
            .map(|index| entry(&format!("App {index:03}"), &format!("app-{index}"), apps::AppKind::Desktop))
            .collect();
        let report = every_app(&catalog, MAX);
        assert_eq!(report.apps.len(), MAX);
        assert_eq!(report.apps[0].name, "App 000");
        assert_eq!(report.note.as_deref(), Some("showing 30 of 200 installed apps; narrow with query or raise max"));
    }

    #[test]
    fn launch_extensions_are_ignored_in_the_filter() {
        let catalog = vec![
            entry("Notepad", "notepad", apps::AppKind::Packaged),
            entry("Paint", "paint", apps::AppKind::Packaged),
        ];
        for query in ["notepad.exe", "Notepad.LNK", "notepad.bat"] {
            assert_eq!(names(&matching(&catalog, without_launch_extension(query), MAX)), ["Notepad"], "{query}");
        }
    }

    #[test]
    fn live_apps_list_and_filter() {
        let all = list(engine(), &request(None, 500)).unwrap();
        assert!(!all.apps.is_empty() && all.apps.len() <= 500);
        assert_eq!(format::windows(&all).lines().count(), all.apps.len() + 1);
        let first = all.apps[0].name.clone();
        let filtered = list(engine(), &request(Some(&first), MAX)).unwrap();
        assert!(filtered.apps.len() <= MAX);
        assert!(filtered.apps.iter().any(|app| app.name == first));
    }
}
