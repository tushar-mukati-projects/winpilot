use std::time::Duration;

use deckhand_core::{
    Result,
    protocol::{AppInfo, AppKind, WindowsArgs, WindowsReport},
};
use deckhand_desktop::apps::{self, AppEntry};

use super::{app_match, launch::without_launch_extension};
use crate::engine::Engine;

const MAX_MATCHES: usize = 30;
const MAX_LISTED: usize = 150;
const FRESH_LISTING: Duration = Duration::from_secs(300);
const RECENT_LISTING: Duration = Duration::from_secs(10);

pub fn apps(engine: &Engine, args: &WindowsArgs) -> Result<WindowsReport> {
    let Some(query) = args.app.as_deref().map(str::trim).filter(|query| !query.is_empty()) else {
        return Ok(every_app(&engine.apps.apps(FRESH_LISTING)?));
    };
    let query = without_launch_extension(query);
    let catalog = engine.apps.apps(FRESH_LISTING)?;
    if app_match::ranked(&catalog, query).is_empty() {
        return Ok(matching(&engine.apps.apps(RECENT_LISTING)?, query));
    }
    Ok(matching(&catalog, query))
}

fn every_app(catalog: &[AppEntry]) -> WindowsReport {
    let mut apps: Vec<AppInfo> = catalog.iter().map(app_info).collect();
    apps.sort_by_cached_key(|app| app.name.to_lowercase());
    let total = apps.len();
    let note = if total > MAX_LISTED {
        format!("showing {MAX_LISTED} of {total} installed apps; narrow with app:\"name\"")
    } else {
        format!("{total} installed apps")
    };
    apps.truncate(MAX_LISTED);
    WindowsReport { apps, note: Some(note), ..WindowsReport::default() }
}

fn matching(catalog: &[AppEntry], query: &str) -> WindowsReport {
    let found = app_match::ranked(catalog, query);
    let note = match found.len() {
        0 => Some(format!("no installed app matches \"{query}\"")),
        count if count > MAX_MATCHES => {
            Some(format!("showing {MAX_MATCHES} of {count} apps matching \"{query}\"; narrow the filter"))
        }
        _ => None,
    };
    let apps = found.into_iter().take(MAX_MATCHES).map(app_info).collect();
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
    use deckhand_core::{format, protocol::WindowAction};

    use super::*;
    use crate::{call_scope::CallScope, test_fixture::engine, window_tool::run};

    fn entry(name: &str, id: &str, kind: apps::AppKind) -> AppEntry {
        AppEntry { name: name.to_string(), id: id.to_string(), kind }
    }

    fn names(report: &WindowsReport) -> Vec<&str> {
        report.apps.iter().map(|app| app.name.as_str()).collect()
    }

    fn apps_args(app: Option<&str>) -> WindowsArgs {
        WindowsArgs { action: WindowAction::Apps, window: None, app: app.map(str::to_string), args: None, rect: None }
    }

    #[test]
    fn entries_convert_with_their_kind() {
        let packaged = app_info(&entry("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App", apps::AppKind::Packaged));
        assert_eq!(packaged.kind, AppKind::Packaged);
        assert_eq!((packaged.name.as_str(), packaged.id.as_str()), ("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App"));
        assert_eq!(app_info(&entry("Tool", "C:\\tool.exe", apps::AppKind::Desktop)).kind, AppKind::Desktop);
    }

    #[test]
    fn matches_are_ranked_and_capped_at_thirty() {
        let mut catalog: Vec<AppEntry> = (0..40)
            .map(|index| entry(&format!("Tool {index}"), &format!("tool-{index}"), apps::AppKind::Desktop))
            .collect();
        catalog.push(entry("Tool", "tool", apps::AppKind::Desktop));
        let report = matching(&catalog, "tool");
        assert_eq!(report.apps.len(), MAX_MATCHES);
        assert_eq!(report.apps[0].name, "Tool");
        assert_eq!(report.note.as_deref(), Some("showing 30 of 41 apps matching \"tool\"; narrow the filter"));
        let few = matching(&catalog, "tool 39");
        assert_eq!(names(&few), ["Tool 39"]);
        assert_eq!(few.note, None);
    }

    #[test]
    fn no_match_says_so() {
        let catalog = vec![entry("Paint", "paint", apps::AppKind::Desktop)];
        let report = matching(&catalog, "qqqq");
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
        let report = every_app(&catalog);
        assert_eq!(names(&report), ["Calculator", "Notepad", "paint"]);
        assert_eq!(report.note.as_deref(), Some("3 installed apps"));
    }

    #[test]
    fn the_full_list_is_capped_with_a_hint_to_filter() {
        let catalog: Vec<AppEntry> = (0..200)
            .map(|index| entry(&format!("App {index:03}"), &format!("app-{index}"), apps::AppKind::Desktop))
            .collect();
        let report = every_app(&catalog);
        assert_eq!(report.apps.len(), MAX_LISTED);
        assert_eq!(report.apps[0].name, "App 000");
        assert_eq!(report.note.as_deref(), Some("showing 150 of 200 installed apps; narrow with app:\"name\""));
    }

    #[test]
    fn launch_extensions_are_ignored_in_the_filter() {
        let catalog = vec![
            entry("Notepad", "notepad", apps::AppKind::Packaged),
            entry("Paint", "paint", apps::AppKind::Packaged),
        ];
        for query in ["notepad.exe", "Notepad.LNK", "notepad.bat"] {
            assert_eq!(names(&matching(&catalog, without_launch_extension(query))), ["Notepad"], "{query}");
        }
    }

    #[test]
    fn live_apps_list_and_filter() {
        let all = run(engine(), &apps_args(None), &CallScope::relaxed()).unwrap();
        assert!(!all.apps.is_empty() && all.apps.len() <= MAX_LISTED);
        assert_eq!(format::windows(&all).lines().count(), all.apps.len() + 1);
        let first = all.apps[0].name.clone();
        let filtered = run(engine(), &apps_args(Some(&first)), &CallScope::relaxed()).unwrap();
        assert!(filtered.apps.len() <= MAX_MATCHES);
        assert!(filtered.apps.iter().any(|app| app.name == first));
        let blank = run(engine(), &apps_args(Some("  ")), &CallScope::relaxed()).unwrap();
        assert_eq!(blank.apps.len(), all.apps.len());
    }
}
