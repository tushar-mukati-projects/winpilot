use crate::{
    diff::Change,
    element::{Element, ElementState, Snapshot, WindowInfo, WindowState},
    error::Error,
    geometry::{Point, Rect},
    matching::is_gap,
    object_marks::without_object_marks,
    recording::{RecordReport, RecordState},
    reports::{
        ActReport, After, AppInfo, AppKind, AppsReport, ClipboardReport, DiffReport, Found, MonitorInfo,
        MonitorsReport, Observation, ScreenshotReport, SearchReport, StatusReport, StepOutcome, TextReport, TextSource,
        Timing, WindowsReport,
    },
    role::Role,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObserveFormat {
    pub all: bool,
    pub coords: bool,
    pub max: usize,
}

impl Default for ObserveFormat {
    fn default() -> Self {
        Self { all: false, coords: false, max: 300 }
    }
}

const NAME_LIMIT: usize = 80;
const VALUE_LIMIT: usize = 60;
const APP_ID_LIMIT: usize = 160;
const MAX_CHANGES: usize = 40;
const MAX_LISTED_WINDOWS: usize = 8;
const RECENT_TIMINGS: usize = 5;
const PASSWORD_MASK: &str = "•••";
const ELLIPSIS: char = '…';
const INDENT: &str = "  ";
const TREE_TRUNCATED: &str = "(tree truncated: window has more elements than the snapshot limit)";
const BYTES_PER_KB: f64 = 1024.0;
const BYTES_PER_MB: f64 = 1024.0 * 1024.0;

pub fn observation(observation: &Observation, format: &ObserveFormat) -> String {
    let snapshot = &observation.snapshot;
    let listed = listed_elements(&snapshot.elements, format.all);
    let total = listed.iter().filter(|&&keep| keep).count();
    let mut lines = vec![observation_header(snapshot)];
    lines.extend(element_lines(&snapshot.elements, &listed, format));
    if total > format.max {
        lines.push(format!("… {} more elements (narrow with query, or raise max)", total - format.max));
    }
    if snapshot.truncated {
        lines.push(TREE_TRUNCATED.to_string());
    }
    lines.extend(window_list("other windows", &observation.others));
    lines.join("\n")
}

pub fn search(report: &SearchReport) -> String {
    let mut lines: Vec<String> = report.matches.iter().map(found_line).collect();
    if lines.is_empty() {
        lines.push("no matches".to_string());
    }
    if report.truncated {
        lines.push("… more matches (raise max)".to_string());
    }
    lines.join("\n")
}

pub fn act(report: &ActReport) -> String {
    let mut lines: Vec<String> = report.steps.iter().map(step_line).collect();
    if report.steps.len() < report.total_steps {
        lines.push(format!("stopped after step {} of {}", report.steps.len(), report.total_steps));
    }
    lines.extend(after_lines(&report.after));
    lines.push(format!("took {} ms", report.elapsed_ms));
    lines.join("\n")
}

pub fn windows(report: &WindowsReport) -> String {
    listed(&report.windows.iter().map(window_line).collect::<Vec<_>>(), "no windows")
}

pub fn monitors(report: &MonitorsReport) -> String {
    listed(&report.monitors.iter().map(monitor_line).collect::<Vec<_>>(), "no monitors")
}

pub fn apps(report: &AppsReport) -> String {
    let mut lines: Vec<String> = report.note.iter().cloned().collect();
    lines.extend(report.apps.iter().map(app_line));
    listed(&lines, "no apps")
}

fn listed(lines: &[String], when_empty: &str) -> String {
    if lines.is_empty() {
        return when_empty.to_string();
    }
    lines.join("\n")
}

fn monitor_line(monitor: &MonitorInfo) -> String {
    let primary = if monitor.primary { " primary" } else { "" };
    format!(
        "monitor {} {} {}%{primary} · work area {}",
        monitor.index,
        size_at(monitor.rect),
        monitor.scale_percent,
        size_at(monitor.work_area)
    )
}

fn app_line(app: &AppInfo) -> String {
    let kind = match app.kind {
        AppKind::Packaged => "packaged",
        AppKind::Desktop => "desktop",
    };
    format!("{} {kind} {}", quote(app.name.trim(), NAME_LIMIT), single_line(app.id.trim(), APP_ID_LIMIT))
}

pub fn text(report: &TextReport) -> String {
    let mut lines = Vec::new();
    if report.source == TextSource::Ocr {
        lines.push("[ocr]");
    }
    lines.push(if report.text.is_empty() { "(no text)" } else { report.text.as_str() });
    if report.truncated {
        lines.push("(truncated)");
    }
    lines.extend(report.note.as_deref());
    lines.join("\n")
}

pub fn clipboard(report: &ClipboardReport) -> String {
    let text = report.text.as_deref().unwrap_or_default();
    if text.is_empty() {
        return "(clipboard has no text)".to_string();
    }
    if report.truncated {
        return format!("{text}\n(truncated)");
    }
    text.to_string()
}

pub fn status(report: &StatusReport) -> String {
    let mut lines = vec![
        format!("version: {}", report.version),
        format!("uptime: {}", seconds(report.uptime_ms)),
        stopped_line(report),
        format!("user idle: {}", report.user_idle_ms.map_or_else(|| "unknown".to_string(), seconds)),
        hotkey_line(report),
        format!("elevated: {}", yes_no(report.elevated)),
        format!("secure desktop: {}", yes_no(report.secure_desktop)),
        format!("worker restarts: {}", report.worker_restarts),
    ];
    lines.extend(recent_line(&report.recent));
    lines.join("\n")
}

pub fn screenshot_caption(report: &ScreenshotReport) -> String {
    let image = report.image;
    let format = report.mime.strip_prefix("image/").unwrap_or(&report.mime);
    let Point { x, y } = report.mapping.origin;
    let quality = report.quality.map(|quality| format!(" · q{quality}")).unwrap_or_default();
    let caption = format!(
        "{image} {}x{} {format}{quality} · scale {:.2} · origin {x},{y} · {} · click points as {{\"x\":..,\"y\":..,\"img\":\"{image}\"}}",
        report.width, report.height, report.mapping.scale, report.source
    );
    match &report.note {
        Some(note) => format!("{caption}\n{note}"),
        None => caption,
    }
}

pub fn record(report: &RecordReport) -> String {
    let mut lines = vec![record_headline(report)];
    lines.extend(report.note.clone());
    lines.join("\n")
}

fn record_headline(report: &RecordReport) -> String {
    let path = report.path.as_deref().unwrap_or("(no file)");
    match report.state {
        RecordState::Idle => "not recording".to_string(),
        RecordState::Starting => {
            "the recording is starting; check again with observe mode=record action=status".to_string()
        }
        RecordState::Recording => recording_line(report, path),
        RecordState::Finished => {
            let frames = frame_count(report.frames);
            format!("saved {path} · {:.1} s · {frames} · {}", report.seconds, byte_size(report.bytes))
        }
    }
}

fn recording_line(report: &RecordReport, path: &str) -> String {
    let mut parts =
        vec![format!("recording to {path}"), format!("{}x{} @ {} fps", report.width, report.height, report.fps)];
    if report.frames > 0 {
        parts.push(format!("{:.1} s", report.seconds));
        parts.push(frame_count(report.frames));
    }
    parts.push("stop with observe mode=record action=stop".to_string());
    parts.join(" · ")
}

fn frame_count(frames: u64) -> String {
    count_noun(usize::try_from(frames).unwrap_or(usize::MAX), "frame")
}

fn byte_size(bytes: u64) -> String {
    let size = bytes as f64;
    if size >= BYTES_PER_MB {
        return format!("{:.1} MB", size / BYTES_PER_MB);
    }
    if size >= BYTES_PER_KB {
        return format!("{:.0} KB", size / BYTES_PER_KB);
    }
    format!("{bytes} B")
}

pub fn error(error: &Error) -> String {
    format!("error[{}]: {}", error.code, error.message)
}

pub fn window_line(window: &WindowInfo) -> String {
    let mut parts = vec![window_tag(window), quote(window.title.trim(), NAME_LIMIT)];
    parts.extend(window_flags(window).map(str::to_string));
    parts.extend(other_desktop(window));
    parts.join(" ")
}

pub fn other_desktop(window: &WindowInfo) -> Option<String> {
    if window.on_current_desktop {
        return None;
    }
    Some(window.desktop.map_or_else(|| "another desktop".to_string(), |number| format!("desktop {number}")))
}

pub fn element_line(element: &Element, coords: bool) -> String {
    let mut parts = vec![element.id.to_string(), element.role.to_string()];
    parts.extend(element_label(element));
    parts.extend(element_value(element));
    parts.extend(element_flags(element).map(str::to_string));
    if coords {
        parts.push(coordinates(element.rect));
    }
    parts.join(" ")
}

fn element_label(element: &Element) -> Option<String> {
    let name = without_object_marks(&element.name);
    let name = name.trim();
    if !name.is_empty() {
        return Some(quote(name, NAME_LIMIT));
    }
    let id = element.automation_id.trim();
    (!id.is_empty()).then(|| format!("#{}", single_line(id, NAME_LIMIT)))
}

fn element_value(element: &Element) -> Option<String> {
    let value = element.value.as_deref().filter(|value| !without_object_marks(value).is_empty())?;
    if element.state.password {
        return Some(format!("value={PASSWORD_MASK}"));
    }
    Some(format!("value={}", quote(value, VALUE_LIMIT)))
}

fn element_flags(element: &Element) -> impl Iterator<Item = &'static str> {
    let state = element.state;
    [
        state.focused.then_some("focused"),
        (!state.enabled).then_some("disabled"),
        state.toggled.map(|on| if on { "checked" } else { "unchecked" }),
        state.expanded.map(|open| if open { "expanded" } else { "collapsed" }),
        (state.selected == Some(true)).then_some("selected"),
        (state.read_only && shows_read_only(element.role)).then_some("readonly"),
        state.password.then_some("password"),
        state.offscreen.then_some("offscreen"),
    ]
    .into_iter()
    .flatten()
}

const fn shows_read_only(role: Role) -> bool {
    matches!(role, Role::Edit | Role::Combobox | Role::Document)
}

fn coordinates(rect: Rect) -> String {
    let Rect { x, y, width, height } = rect;
    format!("@{x},{y},{width}x{height}")
}

fn window_tag(window: &WindowInfo) -> String {
    let process = window.process.trim();
    if process.is_empty() {
        return window.hwnd.to_string();
    }
    format!("{} {}", window.hwnd, single_line(process, NAME_LIMIT))
}

fn window_flags(window: &WindowInfo) -> impl Iterator<Item = &'static str> {
    let placement = match window.state {
        WindowState::Minimized => Some("minimized"),
        WindowState::Maximized => Some("maximized"),
        WindowState::Normal => None,
    };
    [placement, window.elevated.then_some("elevated"), window.foreground.then_some("foreground")].into_iter().flatten()
}

fn window_list(label: &str, windows: &[WindowInfo]) -> Option<String> {
    if windows.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = windows.iter().take(MAX_LISTED_WINDOWS).map(window_line).collect();
    if windows.len() > MAX_LISTED_WINDOWS {
        parts.push(format!("… {} more", windows.len() - MAX_LISTED_WINDOWS));
    }
    Some(format!("{label}: {}", parts.join(", ")))
}

fn observation_header(snapshot: &Snapshot) -> String {
    let focus = snapshot.focused.map(|id| format!(" · focus {id}")).unwrap_or_default();
    format!("window {} {}{focus}", window_line(&snapshot.window), size_at(snapshot.window.rect))
}

fn size_at(rect: Rect) -> String {
    let Rect { x, y, width, height } = rect;
    format!("{width}x{height}@{x},{y}")
}

fn element_lines(elements: &[Element], listed: &[bool], format: &ObserveFormat) -> Vec<String> {
    let mut lines = Vec::new();
    let mut open_depths: Vec<u16> = Vec::new();
    for (element, &keep) in elements.iter().zip(listed) {
        if lines.len() >= format.max {
            break;
        }
        while open_depths.last().is_some_and(|&depth| depth >= element.depth) {
            open_depths.pop();
        }
        if keep {
            lines.push(format!("{}{}", INDENT.repeat(open_depths.len()), element_line(element, format.coords)));
            open_depths.push(element.depth);
        }
    }
    lines
}

fn listed_elements(elements: &[Element], all: bool) -> Vec<bool> {
    if all {
        return elements.iter().map(listed_in_full).collect();
    }
    let base: Vec<bool> = elements.iter().map(listed_by_default).collect();
    let holds_listed = holds_listed_descendant(elements, &base);
    elements
        .iter()
        .zip(base)
        .zip(holds_listed)
        .map(|((element, keep), holds)| keep || (holds && is_named_container(element)))
        .collect()
}

fn listed_in_full(element: &Element) -> bool {
    let identified = has_name(element) || has_value(element) || !element.automation_id.trim().is_empty();
    element.is_visible() && (identified || element.is_actionable())
}

fn listed_by_default(element: &Element) -> bool {
    element.is_visible() && (element.is_actionable() || is_readable_text(element))
}

fn is_readable_text(element: &Element) -> bool {
    matches!(element.role, Role::Text | Role::Document | Role::Edit) && (has_name(element) || has_value(element))
}

fn is_named_container(element: &Element) -> bool {
    element.is_visible() && element.role.is_container() && has_name(element)
}

fn has_name(element: &Element) -> bool {
    !without_object_marks(&element.name).trim().is_empty()
}

fn has_value(element: &Element) -> bool {
    element.value.as_deref().is_some_and(|value| !without_object_marks(value).is_empty())
}

fn holds_listed_descendant(elements: &[Element], listed: &[bool]) -> Vec<bool> {
    let mut holds = vec![false; elements.len()];
    let mut open: Vec<(usize, u16)> = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        while open.last().is_some_and(|&(_, depth)| depth >= element.depth) {
            open.pop();
        }
        if listed.get(index).copied().unwrap_or(false) {
            mark_open_ancestors(&open, &mut holds);
        }
        open.push((index, element.depth));
    }
    holds
}

fn mark_open_ancestors(open: &[(usize, u16)], holds: &mut [bool]) {
    for &(ancestor, _) in open.iter().rev() {
        let Some(slot) = holds.get_mut(ancestor) else { break };
        if *slot {
            break;
        }
        *slot = true;
    }
}

fn found_line(found: &Found) -> String {
    let mut parts = vec![element_line(&found.element, false), window_tag(&found.window)];
    if !found.path.is_empty() {
        let path: Vec<String> = found.path.iter().map(|step| single_line(step.trim(), NAME_LIMIT)).collect();
        parts.push(path.join(" > "));
    }
    parts.join(" · ")
}

fn step_line(outcome: &StepOutcome) -> String {
    let number = outcome.index.saturating_add(1);
    if let Some(failure) = &outcome.error {
        return format!("✗ {number} {} — {}", outcome.step, error(failure));
    }
    let method = outcome.method.as_ref().map(|method| format!(" ({method})")).unwrap_or_default();
    let note = outcome.note.as_ref().map(|note| format!(": {note}")).unwrap_or_default();
    format!("✓ {number} {}{method}{note}", outcome.step)
}

fn after_lines(after: &After) -> Vec<String> {
    match after {
        After::None => Vec::new(),
        After::Diff(report) => diff_lines(report),
        After::Observation(found) => vec![format!("after: {}", observation(found, &ObserveFormat::default()))],
        After::Screenshot(shot) => vec![format!("after: {}", screenshot_caption(shot))],
    }
}

fn diff_lines(report: &DiffReport) -> Vec<String> {
    let window = window_line(&report.window);
    let focus = report.focused.as_ref().map(|element| format!(" · focus {}", element_line(element, false)));
    let focus = focus.unwrap_or_default();
    if report.changes.is_empty() && report.opened.is_empty() && report.closed.is_empty() {
        return vec![format!("after: no visible change in {window}{focus}")];
    }
    let mut lines = vec![format!("after: window {window}{focus}")];
    lines.extend(report.changes.iter().take(MAX_CHANGES).map(change_line));
    if report.changes.len() > MAX_CHANGES {
        lines.push(format!("… {} more changes", report.changes.len() - MAX_CHANGES));
    }
    lines.extend(window_list("opened", &report.opened));
    lines.extend(window_list("closed", &report.closed));
    lines
}

fn change_line(change: &Change) -> String {
    match change {
        Change::Added { element } => format!("+ {}", element_line(element, false)),
        Change::Removed { element } => format!("- {}", element_line(element, false)),
        Change::Changed { before, after } => changed_line(before, after),
    }
}

fn changed_line(before: &Element, after: &Element) -> String {
    let line = format!("~ {}", element_line(after, false));
    let details = change_details(before, after);
    if details.is_empty() {
        return line;
    }
    format!("{line} ({})", details.join(", "))
}

fn change_details(before: &Element, after: &Element) -> Vec<String> {
    let mut details = Vec::new();
    let (old_name, new_name) = (before.name.trim(), after.name.trim());
    if old_name != new_name {
        details.push(format!("name {} → {}", quote(old_name, NAME_LIMIT), quote(new_name, NAME_LIMIT)));
    }
    details.extend(value_change(before, after));
    if before.role != after.role {
        details.push(format!("role {} → {}", before.role, after.role));
    }
    if before.automation_id != after.automation_id {
        let (old_id, new_id) = (&before.automation_id, &after.automation_id);
        details.push(format!("id {} → {}", quote(old_id, NAME_LIMIT), quote(new_id, NAME_LIMIT)));
    }
    details.extend(state_changes(before.state, after.state).map(str::to_string));
    details
}

fn value_change(before: &Element, after: &Element) -> Option<String> {
    let old = before.value.as_deref().unwrap_or_default();
    let new = after.value.as_deref().unwrap_or_default();
    if old == new {
        return None;
    }
    if before.state.password || after.state.password {
        return Some("value changed".to_string());
    }
    Some(format!("value {} → {}", quote(old, VALUE_LIMIT), quote(new, VALUE_LIMIT)))
}

fn state_changes(before: ElementState, after: ElementState) -> impl Iterator<Item = &'static str> {
    [
        flag_change(before.enabled, after.enabled, "now enabled", "now disabled"),
        option_change(before.toggled, after.toggled, ["now checked", "now unchecked", "no longer checkable"]),
        option_change(before.expanded, after.expanded, ["now expanded", "now collapsed", "no longer expandable"]),
        option_change(before.selected, after.selected, ["now selected", "now unselected", "no longer selectable"]),
        flag_change(before.read_only, after.read_only, "now readonly", "now editable"),
        flag_change(before.password, after.password, "now password", "no longer password"),
        flag_change(before.offscreen, after.offscreen, "now offscreen", "now onscreen"),
        flag_change(before.focusable, after.focusable, "now focusable", "no longer focusable"),
    ]
    .into_iter()
    .flatten()
}

fn flag_change(before: bool, after: bool, on: &'static str, off: &'static str) -> Option<&'static str> {
    (before != after).then_some(if after { on } else { off })
}

fn option_change(before: Option<bool>, after: Option<bool>, words: [&'static str; 3]) -> Option<&'static str> {
    let [on, off, gone] = words;
    if before == after {
        return None;
    }
    Some(match after {
        Some(true) => on,
        Some(false) => off,
        None => gone,
    })
}

fn count_noun(count: usize, noun: &str) -> String {
    if count == 1 { format!("1 {noun}") } else { format!("{count} {noun}s") }
}

fn seconds(ms: u64) -> String {
    format!("{:.1} s", ms as f64 / 1000.0)
}

const fn yes_no(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}

fn stopped_line(report: &StatusReport) -> String {
    if report.stopped {
        return format!("stopped: yes — press {} again to resume", report.kill_hotkey);
    }
    "stopped: no".to_string()
}

fn hotkey_line(report: &StatusReport) -> String {
    let suffix = if report.hotkey_registered { "" } else { " (not registered)" };
    format!("kill hotkey: {}{suffix}", report.kill_hotkey)
}

fn recent_line(recent: &[Timing]) -> Option<String> {
    if recent.is_empty() {
        return None;
    }
    let skipped = recent.len().saturating_sub(RECENT_TIMINGS);
    let timings: Vec<String> = recent.iter().skip(skipped).map(timing).collect();
    Some(format!("recent: {}", timings.join(", ")))
}

fn timing(timing: &Timing) -> String {
    let outcome = if timing.ok { "ok" } else { "err" };
    format!("{} {} ms {outcome}", timing.tool, timing.ms)
}

fn quote(text: &str, limit: usize) -> String {
    let flat = collapse(&without_object_marks(text), is_gap);
    let (kept, cut) = truncate(&flat, limit);
    let mut quoted = String::with_capacity(kept.len() + 8);
    quoted.push('"');
    for ch in kept.chars() {
        if matches!(ch, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    if cut {
        quoted.push(ELLIPSIS);
    }
    quoted
}

fn single_line(text: &str, limit: usize) -> String {
    let flat = collapse(&without_object_marks(text), char::is_control);
    let (kept, cut) = truncate(&flat, limit);
    let mut line = kept.to_string();
    if cut {
        line.push(ELLIPSIS);
    }
    line
}

fn collapse(text: &str, is_break: fn(char) -> bool) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut after_gap = false;
    for ch in text.chars() {
        let gap = is_break(ch);
        match (gap, after_gap) {
            (false, _) => collapsed.push(ch),
            (true, false) => collapsed.push(' '),
            (true, true) => {}
        }
        after_gap = gap;
    }
    collapsed
}

fn truncate(text: &str, limit: usize) -> (&str, bool) {
    if text.chars().nth(limit).is_none() {
        return (text, false);
    }
    let cut = text.char_indices().nth(limit.saturating_sub(1)).map_or(0, |(index, _)| index);
    (&text[..cut], true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        element::fixtures::{app_window, element, snapshot, window, with_value},
        error::ErrorCode,
        geometry::ImageMapping,
        protocol::Timing,
        refs::{ElementRef, ImageRef},
    };

    fn notepad_window() -> WindowInfo {
        window("Untitled - Notepad")
    }

    fn with_id(mut element: Element, automation_id: &str) -> Element {
        element.automation_id = automation_id.to_string();
        element
    }

    fn with_state(mut element: Element, state: ElementState) -> Element {
        element.state = state;
        element
    }

    fn enabled() -> ElementState {
        ElementState { enabled: true, ..ElementState::default() }
    }

    fn editor(value: &str) -> Element {
        let mut editor = with_value(element(5, 1, Role::Edit, "Text editor"), value);
        editor.state.focused = true;
        editor
    }

    fn sample_elements() -> Vec<Element> {
        let mut hidden = element(14, 1, Role::Button, "Hidden");
        hidden.state.offscreen = true;
        vec![
            element(1, 0, Role::Window, "Untitled - Notepad"),
            element(2, 1, Role::MenuBar, "Application"),
            element(3, 2, Role::MenuItem, "File"),
            element(4, 2, Role::MenuItem, "Edit"),
            editor("hello"),
            element(6, 1, Role::Pane, ""),
            element(7, 2, Role::Group, "Options"),
            with_state(element(8, 3, Role::Checkbox, "Word wrap"), ElementState { toggled: Some(false), ..enabled() }),
            element(9, 1, Role::StatusBar, "Status"),
            element(10, 2, Role::Text, "Ln 1, Col 6"),
            with_id(element(11, 1, Role::Image, ""), "logo"),
            element(12, 1, Role::Pane, "Empty pane"),
            element(13, 2, Role::Text, ""),
            hidden,
        ]
    }

    fn sample_observation() -> Observation {
        let mut snap = snapshot(sample_elements());
        snap.focused = Some(ElementRef(5));
        Observation {
            snapshot: snap,
            others: vec![app_window(2, "chrome.exe", "Inbox"), app_window(3, "claude-deck.exe", "Claude Deck")],
        }
    }

    fn plain_observation(elements: Vec<Element>) -> Observation {
        Observation { snapshot: snapshot(elements), others: Vec::new() }
    }

    const HEADER: &str = "window w16 notepad.exe \"Untitled - Notepad\" foreground 800x600@0,0";

    fn outcome(index: usize, step: &str, method: Option<&str>, note: Option<&str>) -> StepOutcome {
        StepOutcome {
            index,
            step: step.to_string(),
            method: method.map(str::to_string),
            note: note.map(str::to_string),
            error: None,
        }
    }

    fn failure(index: usize, step: &str, error: Error) -> StepOutcome {
        StepOutcome { error: Some(error), ..outcome(index, step, None, None) }
    }

    fn diff_report(changes: Vec<Change>) -> DiffReport {
        DiffReport { window: notepad_window(), focused: None, changes, opened: Vec::new(), closed: Vec::new() }
    }

    fn act_report(steps: Vec<StepOutcome>, total_steps: usize, after: After) -> ActReport {
        ActReport { steps, total_steps, after, elapsed_ms: 120 }
    }

    fn changed(before: Element, after: Element) -> String {
        change_line(&Change::Changed { before, after })
    }

    fn shot(image: u32, mime: &str, origin: Point, scale: f64) -> ScreenshotReport {
        ScreenshotReport {
            image: ImageRef(image),
            mime: mime.to_string(),
            data: String::new(),
            width: 1280,
            height: 800,
            mapping: ImageMapping { origin, scale },
            source: "dxgi".to_string(),
            quality: (mime == "image/jpeg").then_some(80),
            note: None,
        }
    }

    fn status_report() -> StatusReport {
        StatusReport {
            version: "0.1.0".to_string(),
            uptime_ms: 125_400,
            stopped: false,
            user_idle_ms: Some(3_240),
            kill_hotkey: "ctrl+alt+escape".to_string(),
            hotkey_registered: true,
            elevated: false,
            secure_desktop: false,
            worker_restarts: 1,
            recent: vec![
                Timing { tool: "observe elements".to_string(), ms: 12, ok: true },
                Timing { tool: "act".to_string(), ms: 340, ok: false },
            ],
        }
    }

    #[test]
    fn element_line_base_form() {
        assert_eq!(element_line(&element(12, 0, Role::Button, "Save"), false), "e12 button \"Save\"");
        assert_eq!(element_line(&element(12, 0, Role::Button, "  Save  "), false), "e12 button \"Save\"");
    }

    #[test]
    fn element_line_value() {
        let edit = with_value(element(3, 0, Role::Edit, "Name"), "abc");
        assert_eq!(element_line(&edit, false), "e3 edit \"Name\" value=\"abc\"");
        let empty = with_value(element(3, 0, Role::Edit, "Name"), "");
        assert_eq!(element_line(&empty, false), "e3 edit \"Name\"");
        let spaces = with_value(element(3, 0, Role::Edit, "Name"), "  ");
        assert_eq!(element_line(&spaces, false), "e3 edit \"Name\" value=\" \"");
    }

    #[test]
    fn element_line_falls_back_to_automation_id() {
        assert_eq!(element_line(&with_id(element(4, 0, Role::Edit, ""), "txtName"), false), "e4 edit #txtName");
        assert_eq!(element_line(&with_id(element(4, 0, Role::Edit, "   "), "txtName"), false), "e4 edit #txtName");
        assert_eq!(element_line(&element(5, 0, Role::Edit, ""), false), "e5 edit");
        let unnamed = with_value(element(6, 0, Role::Edit, ""), "typed");
        assert_eq!(element_line(&unnamed, false), "e6 edit value=\"typed\"");
    }

    #[test]
    fn element_line_flags_in_fixed_order() {
        let state = ElementState {
            enabled: false,
            focused: true,
            focusable: true,
            offscreen: true,
            read_only: true,
            password: true,
            toggled: Some(true),
            expanded: Some(false),
            selected: Some(true),
        };
        let everything = with_state(with_value(element(7, 0, Role::Edit, "Pass"), "secret"), state);
        assert_eq!(
            element_line(&everything, false),
            "e7 edit \"Pass\" value=••• focused disabled checked collapsed selected readonly password offscreen"
        );
    }

    #[test]
    fn element_line_tri_state_flags() {
        let unchecked =
            with_state(element(8, 0, Role::Checkbox, "Bold"), ElementState { toggled: Some(false), ..enabled() });
        assert_eq!(element_line(&unchecked, false), "e8 checkbox \"Bold\" unchecked");
        let open = with_state(element(9, 0, Role::TreeItem, "src"), ElementState { expanded: Some(true), ..enabled() });
        assert_eq!(element_line(&open, false), "e9 treeitem \"src\" expanded");
        let unselected =
            with_state(element(10, 0, Role::TabItem, "A"), ElementState { selected: Some(false), ..enabled() });
        assert_eq!(element_line(&unselected, false), "e10 tabitem \"A\"");
    }

    #[test]
    fn readonly_shows_only_for_text_entry_roles() {
        let read_only = ElementState { read_only: true, ..enabled() };
        assert_eq!(
            element_line(&with_state(element(1, 0, Role::Edit, "a"), read_only), false),
            "e1 edit \"a\" readonly"
        );
        let combo = with_state(element(2, 0, Role::Combobox, "b"), read_only);
        assert_eq!(element_line(&combo, false), "e2 combobox \"b\" readonly");
        let document = with_state(element(3, 0, Role::Document, "c"), read_only);
        assert_eq!(element_line(&document, false), "e3 document \"c\" readonly");
        assert_eq!(element_line(&with_state(element(4, 0, Role::Button, "d"), read_only), false), "e4 button \"d\"");
    }

    #[test]
    fn password_hides_only_a_present_value() {
        let password = ElementState { password: true, ..enabled() };
        let empty = with_state(element(1, 0, Role::Edit, "Password"), password);
        assert_eq!(element_line(&empty, false), "e1 edit \"Password\" password");
        let blank = with_state(with_value(element(1, 0, Role::Edit, "Password"), ""), password);
        assert_eq!(element_line(&blank, false), "e1 edit \"Password\" password");
    }

    #[test]
    fn element_line_coordinates() {
        assert_eq!(element_line(&element(12, 0, Role::Button, "Save"), true), "e12 button \"Save\" @10,10,50x20");
        let mut far = element(13, 0, Role::Button, "Left");
        far.rect = Rect::new(-1920, 5, 100, 30);
        far.state.focused = true;
        assert_eq!(element_line(&far, true), "e13 button \"Left\" focused @-1920,5,100x30");
    }

    #[test]
    fn quoting_escapes_and_collapses_control_characters() {
        let tricky = element(1, 0, Role::Text, "a\"b\\c\nd\te");
        assert_eq!(element_line(&tricky, false), r#"e1 text "a\"b\\c d e""#);
        let controls = element(2, 0, Role::Text, "x\u{7}\u{1b}y\u{0}z");
        assert_eq!(element_line(&controls, false), "e2 text \"x y z\"");
        let crlf = with_value(element(3, 0, Role::Edit, "Body"), "one\r\ntwo\rthree");
        assert_eq!(element_line(&crlf, false), r#"e3 edit "Body" value="one two three""#);
    }

    #[test]
    fn object_replacement_marks_are_left_out_of_names_and_values() {
        let marked = with_value(element(3, 0, Role::Text, "\u{FFFC}Reply\u{FFFC}"), "a\u{FFFC}b");
        assert_eq!(element_line(&marked, false), "e3 text \"Reply\" value=\"ab\"");
        let only_marks = with_value(element(4, 0, Role::Text, "\u{FFFC} \u{FFFC}"), "\u{FFFC}");
        assert_eq!(element_line(&only_marks, false), "e4 text");
        assert!(!has_name(&only_marks) && !has_value(&only_marks));
    }

    #[test]
    fn whitespace_runs_collapse_to_single_spaces() {
        let spaced = element(1, 0, Role::Text, "Save \t\n  as");
        assert_eq!(element_line(&spaced, false), "e1 text \"Save as\"");
        let mixed = with_value(element(2, 0, Role::Edit, "Body"), "a\u{a0}\u{2003} b\r\n\r\nc");
        assert_eq!(element_line(&mixed, false), "e2 edit \"Body\" value=\"a b c\"");
        let id = with_id(element(3, 0, Role::Edit, ""), "two  spaces\nand\tbreak");
        assert_eq!(element_line(&id, false), "e3 edit #two  spaces and break");
    }

    #[test]
    fn names_truncate_at_eighty_chars() {
        let exact = "n".repeat(80);
        assert_eq!(element_line(&element(1, 0, Role::Text, &exact), false), format!("e1 text \"{exact}\""));
        let long = "n".repeat(81);
        assert_eq!(element_line(&element(1, 0, Role::Text, &long), false), format!("e1 text \"{}\"…", "n".repeat(79)));
    }

    #[test]
    fn values_truncate_at_sixty_chars() {
        let exact = with_value(element(1, 0, Role::Edit, "v"), &"v".repeat(60));
        assert_eq!(element_line(&exact, false), format!("e1 edit \"v\" value=\"{}\"", "v".repeat(60)));
        let long = with_value(element(1, 0, Role::Edit, "v"), &"v".repeat(200));
        assert_eq!(element_line(&long, false), format!("e1 edit \"v\" value=\"{}\"…", "v".repeat(59)));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let wide = "é".repeat(100);
        assert_eq!(element_line(&element(1, 0, Role::Text, &wide), false), format!("e1 text \"{}\"…", "é".repeat(79)));
        let emoji = with_value(element(2, 0, Role::Edit, "e"), &"😀".repeat(61));
        assert_eq!(element_line(&emoji, false), format!("e2 edit \"e\" value=\"{}\"…", "😀".repeat(59)));
    }

    #[test]
    fn truncated_names_keep_the_marker_outside_the_quotes() {
        let long = format!("{}\"tail", "q".repeat(78));
        let line = element_line(&element(1, 0, Role::Text, &long), false);
        assert_eq!(line, format!("e1 text \"{}\\\"\"…", "q".repeat(78)));
        let window = app_window(5, "app.exe", &"t".repeat(90));
        assert_eq!(window_line(&window), format!("w5 app.exe \"{}\"…", "t".repeat(79)));
    }

    #[test]
    fn escapes_do_not_count_toward_the_limit() {
        let quotes = "\"".repeat(80);
        let line = element_line(&element(1, 0, Role::Text, &quotes), false);
        assert_eq!(line, format!("e1 text \"{}\"", "\\\"".repeat(80)));
    }

    #[test]
    fn window_line_forms() {
        assert_eq!(window_line(&notepad_window()), "w16 notepad.exe \"Untitled - Notepad\" foreground");
        let minimized = WindowInfo {
            state: WindowState::Minimized,
            elevated: true,
            ..app_window(2_032_546, "notepad.exe", "Untitled - Notepad")
        };
        assert_eq!(window_line(&minimized), "w2032546 notepad.exe \"Untitled - Notepad\" minimized elevated");
        let maximized = WindowInfo { state: WindowState::Maximized, ..notepad_window() };
        assert_eq!(window_line(&maximized), "w16 notepad.exe \"Untitled - Notepad\" maximized foreground");
        assert_eq!(window_line(&app_window(5, "", "Title")), "w5 \"Title\"");
        assert_eq!(window_line(&app_window(5, "app.exe", "")), "w5 app.exe \"\"");
        assert_eq!(window_line(&app_window(5, "app.exe", "a\nb")), "w5 app.exe \"a b\"");
    }

    #[test]
    fn windows_on_other_desktops_say_which_one() {
        let elsewhere =
            WindowInfo { desktop: Some(2), on_current_desktop: false, ..app_window(123, "notepad.exe", "Notes") };
        assert_eq!(window_line(&elsewhere), "w123 notepad.exe \"Notes\" desktop 2");
        let unnumbered = WindowInfo { desktop: None, ..elsewhere.clone() };
        assert_eq!(window_line(&unnumbered), "w123 notepad.exe \"Notes\" another desktop");
        let minimized = WindowInfo { state: WindowState::Minimized, ..elsewhere };
        assert_eq!(window_line(&minimized), "w123 notepad.exe \"Notes\" minimized desktop 2");
        let here = WindowInfo { desktop: Some(1), ..app_window(5, "app.exe", "Here") };
        assert_eq!(window_line(&here), "w5 app.exe \"Here\"");
        assert_eq!(other_desktop(&here), None);
    }

    #[test]
    fn observation_default_listing() {
        let expected = [
            "window w16 notepad.exe \"Untitled - Notepad\" foreground 800x600@0,0 · focus e5",
            "e1 window \"Untitled - Notepad\"",
            "  e2 menubar \"Application\"",
            "    e3 menuitem \"File\"",
            "    e4 menuitem \"Edit\"",
            "  e5 edit \"Text editor\" value=\"hello\" focused",
            "  e7 group \"Options\"",
            "    e8 checkbox \"Word wrap\" unchecked",
            "  e9 statusbar \"Status\"",
            "    e10 text \"Ln 1, Col 6\"",
            "other windows: w2 chrome.exe \"Inbox\", w3 claude-deck.exe \"Claude Deck\"",
        ];
        assert_eq!(observation(&sample_observation(), &ObserveFormat::default()), expected.join("\n"));
    }

    #[test]
    fn observation_all_lists_identified_elements() {
        let format = ObserveFormat { all: true, ..ObserveFormat::default() };
        let expected = [
            "window w16 notepad.exe \"Untitled - Notepad\" foreground 800x600@0,0 · focus e5",
            "e1 window \"Untitled - Notepad\"",
            "  e2 menubar \"Application\"",
            "    e3 menuitem \"File\"",
            "    e4 menuitem \"Edit\"",
            "  e5 edit \"Text editor\" value=\"hello\" focused",
            "  e7 group \"Options\"",
            "    e8 checkbox \"Word wrap\" unchecked",
            "  e9 statusbar \"Status\"",
            "    e10 text \"Ln 1, Col 6\"",
            "  e11 image #logo",
            "  e12 pane \"Empty pane\"",
            "other windows: w2 chrome.exe \"Inbox\", w3 claude-deck.exe \"Claude Deck\"",
        ];
        assert_eq!(observation(&sample_observation(), &format), expected.join("\n"));
    }

    #[test]
    fn observation_with_coordinates() {
        let format = ObserveFormat { coords: true, ..ObserveFormat::default() };
        let text = observation(&plain_observation(vec![element(1, 0, Role::Button, "OK")]), &format);
        assert_eq!(text, format!("{HEADER}\ne1 button \"OK\" @10,10,50x20"));
    }

    #[test]
    fn observation_respects_max() {
        let format = ObserveFormat { max: 3, ..ObserveFormat::default() };
        let text = observation(&sample_observation(), &format);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[1..],
            [
                "e1 window \"Untitled - Notepad\"",
                "  e2 menubar \"Application\"",
                "    e3 menuitem \"File\"",
                "… 6 more elements (narrow with query, or raise max)",
                "other windows: w2 chrome.exe \"Inbox\", w3 claude-deck.exe \"Claude Deck\"",
            ]
        );
    }

    #[test]
    fn observation_with_zero_max_lists_nothing() {
        let format = ObserveFormat { max: 0, ..ObserveFormat::default() };
        let text = observation(&plain_observation(vec![element(1, 0, Role::Button, "OK")]), &format);
        assert_eq!(text, format!("{HEADER}\n… 1 more elements (narrow with query, or raise max)"));
    }

    #[test]
    fn observation_notes_a_truncated_tree() {
        let mut found = plain_observation(vec![element(1, 0, Role::Button, "OK")]);
        found.snapshot.truncated = true;
        let text = observation(&found, &ObserveFormat::default());
        assert_eq!(
            text,
            format!("{HEADER}\ne1 button \"OK\"\n(tree truncated: window has more elements than the snapshot limit)")
        );
    }

    #[test]
    fn observation_caps_other_windows_at_eight() {
        let mut found = plain_observation(Vec::new());
        found.others = (1..=10).map(|hwnd| app_window(hwnd, "app.exe", &format!("W{hwnd}"))).collect();
        let text = observation(&found, &ObserveFormat::default());
        let listed: Vec<String> = (1..=8).map(|hwnd| format!("w{hwnd} app.exe \"W{hwnd}\"")).collect();
        assert_eq!(text, format!("{HEADER}\nother windows: {}, … 2 more", listed.join(", ")));
    }

    #[test]
    fn observation_of_an_empty_window_is_just_the_header() {
        assert_eq!(observation(&plain_observation(Vec::new()), &ObserveFormat::default()), HEADER);
    }

    #[test]
    fn hidden_containers_and_unnamed_text_are_skipped() {
        let mut hidden_group = element(1, 0, Role::Group, "Hidden group");
        hidden_group.state.offscreen = true;
        let elements = vec![
            hidden_group,
            element(2, 1, Role::Button, "Inside"),
            element(3, 0, Role::Text, ""),
            with_value(element(4, 0, Role::Text, ""), "42"),
            element(5, 0, Role::Document, "Readme"),
            element(6, 0, Role::Image, "Logo"),
        ];
        let text = observation(&plain_observation(elements), &ObserveFormat::default());
        assert_eq!(text, format!("{HEADER}\ne2 button \"Inside\"\ne4 text value=\"42\"\ne5 document \"Readme\""));
    }

    #[test]
    fn named_containers_need_a_listed_descendant() {
        let elements = vec![
            element(1, 0, Role::Pane, "Outer"),
            element(2, 1, Role::Group, "Middle"),
            element(3, 2, Role::Pane, ""),
            element(4, 3, Role::Link, "Help"),
            element(5, 0, Role::List, "Empty list"),
            element(6, 1, Role::Image, "Decoration"),
            element(7, 0, Role::Image, "Not a container"),
            element(8, 1, Role::Button, "Child of image"),
        ];
        let text = observation(&plain_observation(elements), &ObserveFormat::default());
        let expected = [
            HEADER,
            "e1 pane \"Outer\"",
            "  e2 group \"Middle\"",
            "    e4 link \"Help\"",
            "e8 button \"Child of image\"",
        ];
        assert_eq!(text, expected.join("\n"));
    }

    #[test]
    fn search_report_lines() {
        let report = SearchReport {
            matches: vec![
                Found {
                    element: element(9, 2, Role::Button, "Save"),
                    window: notepad_window(),
                    path: vec!["Untitled - Notepad".to_string(), "Application".to_string()],
                },
                Found {
                    element: element(4, 0, Role::Link, "Help"),
                    window: app_window(7, "chrome.exe", "Docs"),
                    path: Vec::new(),
                },
            ],
            truncated: true,
        };
        assert_eq!(
            search(&report),
            "e9 button \"Save\" · w16 notepad.exe · Untitled - Notepad > Application\ne4 link \"Help\" · w7 chrome.exe\n… more matches (raise max)"
        );
    }

    #[test]
    fn search_report_without_matches() {
        assert_eq!(search(&SearchReport { matches: Vec::new(), truncated: false }), "no matches");
    }

    #[test]
    fn act_report_with_failure_and_diff() {
        let steps = vec![
            outcome(0, "click e12", Some("invoke"), None),
            outcome(1, "type \"hello\"", Some("value"), Some("verified")),
            failure(2, "key ctrl+s", Error::not_found("no element matches name:Save")),
        ];
        let mut report = diff_report(vec![
            Change::Added { element: element(20, 1, Role::Button, "Don't save") },
            Change::Removed { element: element(4, 2, Role::MenuItem, "Edit") },
            Change::Changed { before: editor("a"), after: editor("b") },
        ]);
        report.focused = Some(editor("b"));
        report.opened = vec![app_window(77, "notepad.exe", "Save As")];
        report.closed = vec![app_window(78, "notepad.exe", "Find")];
        let expected = [
            "✓ 1 click e12 (invoke)",
            "✓ 2 type \"hello\" (value): verified",
            "✗ 3 key ctrl+s — error[not_found]: no element matches name:Save",
            "stopped after step 3 of 5",
            "after: window w16 notepad.exe \"Untitled - Notepad\" foreground · focus e5 edit \"Text editor\" value=\"b\" focused",
            "+ e20 button \"Don't save\"",
            "- e4 menuitem \"Edit\"",
            "~ e5 edit \"Text editor\" value=\"b\" focused (value \"a\" → \"b\")",
            "opened: w77 notepad.exe \"Save As\"",
            "closed: w78 notepad.exe \"Find\"",
            "took 120 ms",
        ];
        assert_eq!(act(&act_report(steps, 5, After::Diff(report))), expected.join("\n"));
    }

    #[test]
    fn act_report_note_without_method() {
        let steps = vec![outcome(0, "wait 200", None, Some("capped at 5 s"))];
        assert_eq!(act(&act_report(steps, 1, After::None)), "✓ 1 wait 200: capped at 5 s\ntook 120 ms");
    }

    #[test]
    fn act_report_without_visible_change() {
        let steps = vec![outcome(0, "key tab", Some("keys"), None)];
        let text = act(&act_report(steps, 1, After::Diff(diff_report(Vec::new()))));
        assert_eq!(
            text,
            "✓ 1 key tab (keys)\nafter: no visible change in w16 notepad.exe \"Untitled - Notepad\" foreground\ntook 120 ms"
        );
    }

    #[test]
    fn act_report_without_change_still_names_focus() {
        let mut report = diff_report(Vec::new());
        report.focused = Some(editor(""));
        let text = act(&act_report(Vec::new(), 0, After::Diff(report)));
        assert_eq!(
            text,
            "after: no visible change in w16 notepad.exe \"Untitled - Notepad\" foreground · focus e5 edit \"Text editor\" focused\ntook 120 ms"
        );
    }

    #[test]
    fn act_report_with_only_an_opened_window() {
        let mut report = diff_report(Vec::new());
        report.opened = vec![app_window(77, "notepad.exe", "Save As")];
        let text = act(&act_report(Vec::new(), 0, After::Diff(report)));
        assert_eq!(
            text,
            "after: window w16 notepad.exe \"Untitled - Notepad\" foreground\nopened: w77 notepad.exe \"Save As\"\ntook 120 ms"
        );
    }

    #[test]
    fn act_report_caps_changes_at_forty() {
        let changes = (1..=45).map(|id| Change::Added { element: element(id, 0, Role::Button, "B") }).collect();
        let text = act(&act_report(Vec::new(), 0, After::Diff(diff_report(changes))));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 43);
        assert_eq!(lines[40], "+ e40 button \"B\"");
        assert_eq!(lines[41], "… 5 more changes");
        assert_eq!(lines[42], "took 120 ms");
    }

    #[test]
    fn act_report_with_observation() {
        let after = After::Observation(plain_observation(vec![element(1, 0, Role::Button, "OK")]));
        let text = act(&act_report(vec![outcome(0, "click e1", Some("mouse"), None)], 1, after));
        assert_eq!(text, format!("✓ 1 click e1 (mouse)\nafter: {HEADER}\ne1 button \"OK\"\ntook 120 ms"));
    }

    #[test]
    fn act_report_with_screenshot() {
        let after = After::Screenshot(shot(3, "image/jpeg", Point::new(0, 0), 2.0));
        let text = act(&act_report(Vec::new(), 0, after));
        assert_eq!(
            text,
            "after: i3 1280x800 jpeg · q80 · scale 2.00 · origin 0,0 · dxgi · click points as {\"x\":..,\"y\":..,\"img\":\"i3\"}\ntook 120 ms"
        );
    }

    #[test]
    fn act_report_stopped_before_any_step() {
        let text = act(&act_report(Vec::new(), 2, After::None));
        assert_eq!(text, "stopped after step 0 of 2\ntook 120 ms");
    }

    #[test]
    fn changed_lines_describe_name_and_toggle() {
        let before = element(1, 0, Role::Text, "x");
        assert_eq!(changed(before.clone(), element(1, 0, Role::Text, "y")), "~ e1 text \"y\" (name \"x\" → \"y\")");
        let checked =
            with_state(element(2, 0, Role::Checkbox, "Bold"), ElementState { toggled: Some(true), ..enabled() });
        let unset = element(2, 0, Role::Checkbox, "Bold");
        assert_eq!(changed(unset, checked.clone()), "~ e2 checkbox \"Bold\" checked (now checked)");
        let unchecked =
            with_state(element(2, 0, Role::Checkbox, "Bold"), ElementState { toggled: Some(false), ..enabled() });
        assert_eq!(changed(checked.clone(), unchecked), "~ e2 checkbox \"Bold\" unchecked (now unchecked)");
        assert_eq!(
            changed(checked, element(2, 0, Role::Checkbox, "Bold")),
            "~ e2 checkbox \"Bold\" (no longer checkable)"
        );
    }

    #[test]
    fn changed_lines_combine_details_in_order() {
        let before = with_id(with_value(element(3, 0, Role::Text, "a"), "1"), "x");
        let mut after = with_id(with_value(element(3, 0, Role::Edit, "b"), "2"), "y");
        after.state.enabled = false;
        assert_eq!(
            changed(before, after),
            "~ e3 edit \"b\" value=\"2\" disabled (name \"a\" → \"b\", value \"1\" → \"2\", role text → edit, id \"x\" → \"y\", now disabled)"
        );
    }

    #[test]
    fn changed_lines_cover_every_state_word() {
        let cases: [(ElementState, ElementState, &str); 12] = [
            (ElementState::default(), enabled(), "now enabled"),
            (enabled(), ElementState { expanded: Some(true), ..enabled() }, "now expanded"),
            (enabled(), ElementState { expanded: Some(false), ..enabled() }, "now collapsed"),
            (ElementState { expanded: Some(true), ..enabled() }, enabled(), "no longer expandable"),
            (enabled(), ElementState { selected: Some(true), ..enabled() }, "now selected"),
            (
                ElementState { selected: Some(true), ..enabled() },
                ElementState { selected: Some(false), ..enabled() },
                "now unselected",
            ),
            (ElementState { selected: Some(true), ..enabled() }, enabled(), "no longer selectable"),
            (enabled(), ElementState { read_only: true, ..enabled() }, "now readonly"),
            (ElementState { read_only: true, ..enabled() }, enabled(), "now editable"),
            (enabled(), ElementState { offscreen: true, ..enabled() }, "now offscreen"),
            (ElementState { offscreen: true, ..enabled() }, enabled(), "now onscreen"),
            (enabled(), ElementState { focusable: true, ..enabled() }, "now focusable"),
        ];
        for (before, after, word) in cases {
            let line = changed(
                with_state(element(1, 0, Role::Pane, "P"), before),
                with_state(element(1, 0, Role::Pane, "P"), after),
            );
            assert!(line.ends_with(&format!("({word})")), "{line}");
        }
        let password = changed(
            element(1, 0, Role::Edit, "P"),
            with_state(element(1, 0, Role::Edit, "P"), ElementState { password: true, ..enabled() }),
        );
        assert_eq!(password, "~ e1 edit \"P\" password (now password)");
        let unfocusable = changed(
            with_state(element(1, 0, Role::Pane, "P"), ElementState { focusable: true, ..enabled() }),
            element(1, 0, Role::Pane, "P"),
        );
        assert_eq!(unfocusable, "~ e1 pane \"P\" (no longer focusable)");
    }

    #[test]
    fn changed_password_values_stay_hidden() {
        let secret = ElementState { password: true, ..enabled() };
        let before = with_state(with_value(element(1, 0, Role::Edit, "Password"), "old"), secret);
        let after = with_state(with_value(element(1, 0, Role::Edit, "Password"), "new"), secret);
        assert_eq!(changed(before, after), "~ e1 edit \"Password\" value=••• password (value changed)");
    }

    #[test]
    fn changed_value_from_none_to_empty_has_no_detail() {
        let before = element(1, 0, Role::Edit, "Field");
        let after = with_value(element(1, 0, Role::Edit, "Field"), "");
        assert_eq!(changed(before, after), "~ e1 edit \"Field\"");
    }

    #[test]
    fn changed_value_from_none_reads_as_empty() {
        let before = element(1, 0, Role::Edit, "Field");
        let after = with_value(element(1, 0, Role::Edit, "Field"), "x");
        assert_eq!(changed(before, after), "~ e1 edit \"Field\" value=\"x\" (value \"\" → \"x\")");
    }

    #[test]
    fn windows_report_list() {
        let report = WindowsReport { windows: vec![notepad_window(), app_window(2, "chrome.exe", "Inbox")] };
        assert_eq!(windows(&report), "w16 notepad.exe \"Untitled - Notepad\" foreground\nw2 chrome.exe \"Inbox\"");
    }

    #[test]
    fn empty_lists_say_so() {
        assert_eq!(windows(&WindowsReport::default()), "no windows");
        assert_eq!(monitors(&MonitorsReport::default()), "no monitors");
        assert_eq!(apps(&AppsReport::default()), "no apps");
        let note_only = AppsReport { note: Some("3 installed apps".to_string()), apps: Vec::new() };
        assert_eq!(apps(&note_only), "3 installed apps");
    }

    fn monitor(index: u32, rect: Rect, work_area: Rect, scale_percent: u32, primary: bool) -> MonitorInfo {
        MonitorInfo { index, rect, work_area, scale_percent, primary }
    }

    #[test]
    fn monitors_are_one_line_each() {
        let primary = monitor(0, Rect::new(0, 0, 2560, 1600), Rect::new(0, 0, 2560, 1552), 125, true);
        let left = monitor(1, Rect::new(-1920, 200, 1920, 1080), Rect::new(-1920, 200, 1920, 1040), 100, false);
        let report = MonitorsReport { monitors: vec![primary, left] };
        assert_eq!(
            monitors(&report),
            "monitor 0 2560x1600@0,0 125% primary · work area 2560x1552@0,0\n\
             monitor 1 1920x1080@-1920,200 100% · work area 1920x1040@-1920,200"
        );
    }

    fn app(name: &str, id: &str, kind: AppKind) -> AppInfo {
        AppInfo { name: name.to_string(), id: id.to_string(), kind }
    }

    #[test]
    fn apps_are_one_line_each_after_the_note() {
        let report = AppsReport {
            note: Some("showing 2 of 40 apps matching \"note\"".to_string()),
            apps: vec![
                app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", AppKind::Packaged),
                app("Notepad++ \"64\"", "{6D809377}\\Notepad++\\notepad++.exe", AppKind::Desktop),
            ],
        };
        assert_eq!(
            apps(&report),
            "showing 2 of 40 apps matching \"note\"\n\
             \"Notepad\" packaged Microsoft.WindowsNotepad_8wekyb3d8bbwe!App\n\
             \"Notepad++ \\\"64\\\"\" desktop {6D809377}\\Notepad++\\notepad++.exe"
        );
    }

    #[test]
    fn long_app_ids_are_shortened() {
        let line = app_line(&app("Tool", &"x".repeat(400), AppKind::Desktop));
        assert!(line.ends_with('…'));
        assert_eq!(line.chars().count(), "\"Tool\" desktop ".chars().count() + APP_ID_LIMIT);
    }

    #[test]
    fn text_report_forms() {
        let uia =
            TextReport { text: "hello\nworld".to_string(), source: TextSource::Uia, truncated: false, note: None };
        assert_eq!(text(&uia), "hello\nworld");
        let ocr = TextReport { text: "hello".to_string(), source: TextSource::Ocr, truncated: true, note: None };
        assert_eq!(text(&ocr), "[ocr]\nhello\n(truncated)");
        let empty = TextReport { text: String::new(), source: TextSource::Uia, truncated: false, note: None };
        assert_eq!(text(&empty), "(no text)");
        let noted = TextReport { note: Some("the overlay may appear".to_string()), ..ocr };
        assert_eq!(text(&noted), "[ocr]\nhello\n(truncated)\nthe overlay may appear");
    }

    #[test]
    fn clipboard_report_forms() {
        let report = |text: Option<&str>, truncated| ClipboardReport { text: text.map(str::to_string), truncated };
        assert_eq!(clipboard(&report(Some("copied"), false)), "copied");
        assert_eq!(clipboard(&report(Some("cop"), true)), "cop\n(truncated)");
        assert_eq!(clipboard(&report(None, false)), "(clipboard has no text)");
        assert_eq!(clipboard(&report(Some(""), false)), "(clipboard has no text)");
    }

    #[test]
    fn status_report_running() {
        let expected = [
            "version: 0.1.0",
            "uptime: 125.4 s",
            "stopped: no",
            "user idle: 3.2 s",
            "kill hotkey: ctrl+alt+escape",
            "elevated: no",
            "secure desktop: no",
            "worker restarts: 1",
            "recent: observe elements 12 ms ok, act 340 ms err",
        ];
        assert_eq!(status(&status_report()), expected.join("\n"));
    }

    #[test]
    fn status_report_stopped() {
        let report = StatusReport {
            stopped: true,
            user_idle_ms: None,
            hotkey_registered: false,
            elevated: true,
            secure_desktop: true,
            recent: Vec::new(),
            ..status_report()
        };
        let expected = [
            "version: 0.1.0",
            "uptime: 125.4 s",
            "stopped: yes — press ctrl+alt+escape again to resume",
            "user idle: unknown",
            "kill hotkey: ctrl+alt+escape (not registered)",
            "elevated: yes",
            "secure desktop: yes",
            "worker restarts: 1",
        ];
        assert_eq!(status(&report), expected.join("\n"));
    }

    #[test]
    fn status_report_shows_the_last_five_timings() {
        let recent = (1..=7).map(|ms| Timing { tool: format!("t{ms}"), ms, ok: true }).collect();
        let text = status(&StatusReport { recent, ..status_report() });
        assert!(text.ends_with("recent: t3 3 ms ok, t4 4 ms ok, t5 5 ms ok, t6 6 ms ok, t7 7 ms ok"), "{text}");
    }

    #[test]
    fn screenshot_caption_forms() {
        assert_eq!(
            screenshot_caption(&shot(3, "image/jpeg", Point::new(0, 0), 2.0)),
            "i3 1280x800 jpeg · q80 · scale 2.00 · origin 0,0 · dxgi · click points as {\"x\":..,\"y\":..,\"img\":\"i3\"}"
        );
        let png = ScreenshotReport {
            width: 640,
            height: 400,
            source: "window w16".to_string(),
            ..shot(7, "image/png", Point::new(-1920, 0), 1.5)
        };
        assert_eq!(
            screenshot_caption(&png),
            "i7 640x400 png · scale 1.50 · origin -1920,0 · window w16 · click points as {\"x\":..,\"y\":..,\"img\":\"i7\"}"
        );
        let odd = shot(1, "application/octet-stream", Point::new(5, 6), 1.0);
        assert!(screenshot_caption(&odd).starts_with("i1 1280x800 application/octet-stream · scale 1.00 · origin 5,6"));
        let sharp = ScreenshotReport { quality: Some(95), ..shot(2, "image/jpeg", Point::new(0, 0), 1.0) };
        assert!(screenshot_caption(&sharp).starts_with("i2 1280x800 jpeg · q95 · scale 1.00"));
        let noted = ScreenshotReport { note: Some("the overlay may appear".to_string()), ..sharp };
        assert!(screenshot_caption(&noted).ends_with("\"i2\"}\nthe overlay may appear"));
    }

    fn recording(state: RecordState) -> RecordReport {
        RecordReport {
            state,
            path: Some("C:\\Videos\\Deckhand\\deckhand-20261011-101500.mp4".to_string()),
            seconds: 0.0,
            frames: 0,
            bytes: 0,
            width: 1920,
            height: 1200,
            fps: 30,
            note: None,
        }
    }

    #[test]
    fn record_report_forms() {
        assert_eq!(record(&RecordReport::idle()), "not recording");
        assert_eq!(
            record(&RecordReport::starting()),
            "the recording is starting; check again with observe mode=record action=status"
        );
        assert_eq!(
            record(&recording(RecordState::Recording)),
            "recording to C:\\Videos\\Deckhand\\deckhand-20261011-101500.mp4 · 1920x1200 @ 30 fps · stop with observe mode=record action=stop"
        );
        let running = RecordReport { seconds: 4.25, frames: 128, ..recording(RecordState::Recording) };
        let text = record(&running);
        assert!(
            text.ends_with(
                ".mp4 · 1920x1200 @ 30 fps · 4.2 s · 128 frames · stop with observe mode=record action=stop"
            ),
            "{text}"
        );
        let saved = RecordReport {
            path: Some("C:\\clips\\x.mp4".to_string()),
            seconds: 12.4,
            frames: 372,
            bytes: 4_299_161,
            note: Some("dropped 3 of 375 frames because encoding fell behind".to_string()),
            ..recording(RecordState::Finished)
        };
        assert_eq!(
            record(&saved),
            "saved C:\\clips\\x.mp4 · 12.4 s · 372 frames · 4.1 MB\ndropped 3 of 375 frames because encoding fell behind"
        );
    }

    #[test]
    fn byte_sizes_pick_a_readable_unit() {
        assert_eq!(byte_size(0), "0 B");
        assert_eq!(byte_size(1023), "1023 B");
        assert_eq!(byte_size(1024), "1 KB");
        assert_eq!(byte_size(700 * 1024), "700 KB");
        assert_eq!(byte_size(1024 * 1024), "1.0 MB");
        let one_frame = RecordReport { frames: 1, bytes: 2048, ..recording(RecordState::Finished) };
        assert!(record(&one_frame).ends_with("0.0 s · 1 frame · 2 KB"));
    }

    #[test]
    fn error_line() {
        assert_eq!(
            error(&Error::not_found("no element matches name:Save")),
            "error[not_found]: no element matches name:Save"
        );
        let invalid = Error::new(ErrorCode::InvalidArgument, "unknown key 'zz'");
        assert_eq!(error(&invalid), "error[invalid_argument]: unknown key 'zz'");
    }

    #[test]
    fn observe_format_defaults() {
        assert_eq!(ObserveFormat::default(), ObserveFormat { all: false, coords: false, max: 300 });
    }
}
