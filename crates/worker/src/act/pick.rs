mod dialog;
mod dialog_kind;
mod folder;
mod message_box;
mod selection;

use std::{collections::HashSet, path::Path, time::Duration};

use deckhand_core::{
    ElementRef, Error, ErrorCode, Result, TargetSpec, WindowInfo, WindowRef,
    protocol::{Step, pick_wait},
};

use self::{
    dialog::FileDialog,
    dialog_kind::DialogKind,
    folder::{Destination, folder_ready, listed_files},
    message_box::Prompt,
    selection::{Requested, Selection},
};
use super::{click_target, done::Done, foreground::owner_chain, summary::file_count};
use crate::{
    app_windows::app_windows, call_scope::CallScope, default_window::default_window, engine::Engine, guard::Guard,
    overlay, until::until, watch::Watch, window_classes::is_dialog,
};

const ALREADY_OPEN_WAIT: Duration = Duration::from_secs(1);
const CLOSE_WAIT: Duration = Duration::from_secs(5);
const FOLDER_WAIT: Duration = Duration::from_secs(5);

pub(crate) struct PickRequest<'a> {
    pub(crate) paths: &'a [String],
    pub(crate) open_with: Option<&'a TargetSpec>,
    overwrite: bool,
    pub(crate) timeout_ms: Option<u64>,
}

impl<'a> TryFrom<&'a Step> for PickRequest<'a> {
    type Error = Error;

    fn try_from(step: &'a Step) -> Result<Self> {
        let Step::Pick { paths, open_with, overwrite, timeout_ms } = step else {
            return Err(Error::invalid(format!("{} is not a pick step", step.kind())));
        };
        Ok(Self { paths, open_with: open_with.as_ref(), overwrite: *overwrite, timeout_ms: *timeout_ms })
    }
}

struct Opened {
    dialog: FileDialog,
    ours: bool,
}

enum Outcome {
    Closed,
    Prompt(WindowInfo),
}

pub(crate) fn check_paths(paths: &[String], guard: &Guard) -> Result<()> {
    Selection::parse(paths, |path| guard.checked_path(path))?.check_folder()
}

pub(crate) fn step(engine: &Engine, request: &PickRequest<'_>, step: &Step, call: &CallScope) -> Result<Done> {
    let selection = Selection::parse(request.paths, |path| engine.guard.checked_path(path))?;
    selection.check_folder()?;
    let requested = selection.requested();
    engine.guard.before_input(None, call)?;
    let opened = open_dialog(engine, request, step, call)?;
    overlay::begin(engine, &opened.dialog.window, call);
    let picked = pick(engine, &opened.dialog, &selection, &requested, request.overwrite, call);
    if picked.is_err() && opened.ours {
        abandon(engine, &opened.dialog);
    }
    picked?;
    Ok(Done::noting(picked_note(opened.dialog.kind, &selection.files)))
}

fn picked_note(kind: DialogKind, files: &[String]) -> String {
    let shown: Vec<&str> = match kind {
        DialogKind::Open => files.iter().map(|path| base_name(path)).collect(),
        DialogKind::Save => files.iter().map(String::as_str).collect(),
    };
    format!("picked {} in the {} dialog: {}", file_count(files.len()), kind.name(), shown.join(", "))
}

fn base_name(path: &str) -> &str {
    Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path)
}

fn pick(
    engine: &Engine,
    dialog: &FileDialog,
    selection: &Selection,
    requested: &Requested,
    overwrite: bool,
    call: &CallScope,
) -> Result<()> {
    selection.check_for(dialog.kind)?;
    fill(engine, dialog, selection, call)?;
    confirm(engine, dialog, requested, overwrite, call)
}

fn open_dialog(engine: &Engine, request: &PickRequest<'_>, step: &Step, call: &CallScope) -> Result<Opened> {
    let Some(opener) = request.open_with else {
        return existing_dialog(engine, call).map(|dialog| Opened { dialog, ours: false });
    };
    let existing: HashSet<WindowRef> = dialog_windows(engine)?.iter().map(|window| window.hwnd).collect();
    let clicked = click_target(engine, opener, step, call)?;
    let timeout = pick_wait(request.timeout_ms);
    let fresh = |window: &WindowInfo| !existing.contains(&window.hwnd) && belongs_to(window, clicked.hwnd);
    let found = until(engine, call, timeout, &Watch::window(&clicked), || find_dialog(engine, fresh))?;
    let dialog = found.ok_or_else(|| not_opened(opener, timeout))?;
    Ok(Opened { dialog, ours: true })
}

fn existing_dialog(engine: &Engine, call: &CallScope) -> Result<FileDialog> {
    let app = default_window(engine)?;
    let owned = |window: &WindowInfo| belongs_to(window, app.hwnd);
    if let Some(dialog) = until(engine, call, ALREADY_OPEN_WAIT, &Watch::window(&app), || find_dialog(engine, owned))? {
        return Ok(dialog);
    }
    if find_dialog(engine, |_| true)?.is_some() {
        return Err(Error::not_found(format!("no file dialog belongs to {} ({})", app.hwnd, app.process)));
    }
    Err(Error::not_found("no file dialog is open; pass open_with to click the button that opens one"))
}

fn belongs_to(dialog: &WindowInfo, window: WindowRef) -> bool {
    dialog.hwnd == window || owner_chain(Some(dialog.hwnd)).contains(&window)
}

fn not_opened(opener: &TargetSpec, timeout: Duration) -> Error {
    Error::timeout(format!("clicking {opener} did not open a file dialog within {} ms", timeout.as_millis()))
}

fn find_dialog(engine: &Engine, wanted: impl Fn(&WindowInfo) -> bool) -> Result<Option<FileDialog>> {
    let candidates = dialog_windows(engine)?.into_iter().filter(|window| wanted(window));
    for window in candidates.filter(|window| !engine.host.contains(window.pid)) {
        let Ok(snapshot) = engine.snapshot(&window) else { continue };
        if let Some(found) = dialog::recognise(&snapshot) {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn dialog_windows(engine: &Engine) -> Result<Vec<WindowInfo>> {
    Ok(app_windows(engine)?.into_iter().filter(is_dialog).collect())
}

fn fill(engine: &Engine, dialog: &FileDialog, selection: &Selection, call: &CallScope) -> Result<()> {
    if let [file] = selection.files.as_slice() {
        return enter(engine, dialog, file);
    }
    let destination = Destination {
        folder: selection.folder_text(),
        names: selection.file_names(),
        listed_before: listed_files(&engine.snapshot(&dialog.window)?.elements),
    };
    enter(engine, dialog, &destination.folder)?;
    let in_folder = await_folder(engine, dialog, &destination, call)?;
    enter(engine, &in_folder, &selection.quoted_names())
}

fn await_folder(
    engine: &Engine,
    dialog: &FileDialog,
    destination: &Destination,
    call: &CallScope,
) -> Result<FileDialog> {
    let found = until(engine, call, FOLDER_WAIT, &Watch::window(&dialog.window), || {
        let snapshot = engine.snapshot(&dialog.window)?;
        if !folder_ready(&snapshot, dialog.name_box, destination) {
            return Ok(None);
        }
        Ok(dialog::recognise(&snapshot))
    })?;
    found.ok_or_else(|| {
        Error::timeout(format!(
            "the file dialog did not show {} after it was asked to; nothing was picked",
            destination.folder
        ))
    })
}

fn enter(engine: &Engine, dialog: &FileDialog, text: &str) -> Result<()> {
    set_verified(engine, dialog.name_box, text)?;
    press(engine, dialog.confirm)
}

fn set_verified(engine: &Engine, name_box: ElementRef, text: &str) -> Result<()> {
    let wanted = text.to_string();
    let value = engine.uia.run(move |uia| {
        uia.set_value(name_box, &wanted)?;
        uia.value(name_box)
    })?;
    if value.as_deref().map(str::trim) != Some(text.trim()) {
        return Err(Error::unsupported("the file name box did not accept the path"));
    }
    Ok(())
}

fn press(engine: &Engine, button: ElementRef) -> Result<()> {
    let pressed = engine.uia.run(move |uia| match uia.invoke(button) {
        Err(error) if error.code == ErrorCode::Unsupported => uia.default_action(button),
        result => result,
    });
    match pressed {
        Err(error) if error.code == ErrorCode::TargetNotResponding => Ok(()),
        result => result,
    }
}

fn confirm(
    engine: &Engine,
    dialog: &FileDialog,
    requested: &Requested,
    overwrite: bool,
    call: &CallScope,
) -> Result<()> {
    let prompt = match await_outcome(engine, dialog, None, call)? {
        Some(Outcome::Closed) => return Ok(()),
        Some(Outcome::Prompt(prompt)) => prompt,
        None => return Err(still_open(dialog)),
    };
    match message_box::classify(&engine.snapshot(&prompt)?, dialog.kind, requested) {
        Prompt::Replace { yes, .. } if overwrite => replace(engine, dialog, &prompt, yes, call),
        Prompt::Replace { no, .. } => {
            press(engine, no)?;
            Err(Error::new(ErrorCode::NeedsUser, "the file exists; pass overwrite:true to replace it"))
        }
        Prompt::Question { text, decline } => {
            answer(engine, decline)?;
            Err(Error::new(
                ErrorCode::NeedsUser,
                format!("the file dialog asked: {text}; Deckhand declined, so nothing was picked"),
            ))
        }
        Prompt::Message { text, dismiss } => {
            answer(engine, dismiss)?;
            Err(Error::invalid(format!("the file dialog said: {text}")))
        }
    }
}

fn answer(engine: &Engine, button: Option<ElementRef>) -> Result<()> {
    button.map_or(Ok(()), |button| press(engine, button))
}

fn replace(engine: &Engine, dialog: &FileDialog, prompt: &WindowInfo, yes: ElementRef, call: &CallScope) -> Result<()> {
    press(engine, yes)?;
    match await_outcome(engine, dialog, Some(prompt.hwnd), call)? {
        Some(Outcome::Closed) => Ok(()),
        _ => Err(still_open(dialog)),
    }
}

fn await_outcome(
    engine: &Engine,
    dialog: &FileDialog,
    answered: Option<WindowRef>,
    call: &CallScope,
) -> Result<Option<Outcome>> {
    let hwnd = dialog.window.hwnd;
    until(engine, call, CLOSE_WAIT, &Watch::window(&dialog.window), || {
        if !deckhand_desktop::is_window(hwnd) {
            return Ok(Some(Outcome::Closed));
        }
        Ok(new_prompt(engine, hwnd, answered)?.map(Outcome::Prompt))
    })
}

fn new_prompt(engine: &Engine, owner: WindowRef, answered: Option<WindowRef>) -> Result<Option<WindowInfo>> {
    let mut prompts = dialog_windows(engine)?.into_iter().filter(|window| Some(window.hwnd) != answered);
    Ok(prompts.find(|window| deckhand_desktop::owner_of(window.hwnd) == Some(owner)))
}

fn abandon(engine: &Engine, dialog: &FileDialog) {
    let Some(cancel) = dialog.cancel.filter(|_| deckhand_desktop::is_window(dialog.window.hwnd)) else { return };
    if let Err(error) = press(engine, cancel) {
        eprintln!("deckhand worker: could not cancel the file dialog Deckhand opened: {error}");
    }
}

fn still_open(dialog: &FileDialog) -> Error {
    Error::timeout(format!("the {} dialog is still open after confirming", dialog.kind.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::{FixtureApp, TempFiles, engine, temp_file, temp_path};

    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    fn pick_step(paths: Vec<String>, open_with: Option<TargetSpec>, overwrite: bool) -> Step {
        Step::Pick { paths, open_with, overwrite, timeout_ms: Some(10_000) }
    }

    fn run_pick(step: &Step) -> Result<Done> {
        super::step(engine(), &PickRequest::try_from(step)?, step, &CallScope::relaxed())
    }

    fn pick(app: &FixtureApp, button: u32, paths: Vec<String>, overwrite: bool) -> Result<Done> {
        let opener = TargetSpec::Text(app.selector(&format!("id:{button}")));
        run_pick(&pick_step(paths, Some(opener), overwrite))
    }

    #[test]
    fn the_note_names_open_files_and_save_paths() {
        let files = vec!["C:\\Users\\me\\a.txt".to_string(), "C:/temp/b.txt".to_string()];
        assert_eq!(picked_note(DialogKind::Open, &files), "picked 2 files in the Open dialog: a.txt, b.txt");
        let saved = vec!["C:\\temp\\out.txt".to_string()];
        assert_eq!(picked_note(DialogKind::Save, &saved), "picked 1 file in the Save dialog: C:\\temp\\out.txt");
    }

    #[test]
    fn only_pick_steps_become_pick_requests() {
        let request_step = pick_step(vec!["C:\\a.txt".to_string()], None, true);
        let request = PickRequest::try_from(&request_step).unwrap();
        assert_eq!((request.paths.len(), request.overwrite), (1, true));
        let other: Step = serde_json::from_str(r#"{"do":"wait","ms":5}"#).unwrap();
        assert!(PickRequest::try_from(&other).is_err());
    }

    #[test]
    fn a_dialog_belongs_to_its_own_window_and_its_owners_only() {
        let dialog = crate::test_fixture::window(0, "app.exe", "Open", deckhand_core::Rect::default());
        assert!(belongs_to(&dialog, WindowRef(0)));
        assert!(!belongs_to(&dialog, WindowRef(77)));
    }

    #[test]
    fn invalid_paths_fail_before_any_input() {
        let code = |paths: Vec<String>| run_pick(&pick_step(paths, None, false)).unwrap_err().code;
        assert_eq!(code(vec!["relative.txt".to_string()]), ErrorCode::InvalidArgument);
        assert_eq!(code(Vec::new()), ErrorCode::InvalidArgument);
        assert_eq!(code(vec!["C:\\Data\\Report.lnk".to_string()]), ErrorCode::InvalidArgument);
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn opens_one_file() {
        let file = temp_file("one.txt");
        let _cleanup = TempFiles(vec![file.clone()]);
        let Some(app) = FixtureApp::launch() else { return };
        let done = pick(&app, 2011, vec![text(&file)], false).unwrap();
        assert!(done.note.unwrap().starts_with("picked 1 file in the Open dialog"));
        let opened = app.settled_text(2012, |shown| shown.contains(&text(&file)));
        assert!(opened.contains(&text(&file)), "{opened}");
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn opens_two_files_from_one_folder() {
        let first = temp_file("first.txt");
        let second = temp_file("second.txt");
        let _cleanup = TempFiles(vec![first.clone(), second.clone()]);
        let Some(app) = FixtureApp::launch() else { return };
        let done = pick(&app, 2011, vec![text(&first), text(&second)], false).unwrap();
        assert!(done.note.unwrap().starts_with("picked 2 files"));
        let both = |shown: &str| shown.contains("first.txt") && shown.contains("second.txt");
        let opened = app.settled_text(2012, both);
        assert!(both(&opened), "{opened}");
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn saves_to_a_new_path() {
        let target = temp_path("saved.txt");
        let _cleanup = TempFiles(vec![target.clone()]);
        let Some(app) = FixtureApp::launch() else { return };
        let done = pick(&app, 2013, vec![text(&target)], false).unwrap();
        assert!(done.note.unwrap().contains("in the Save dialog"));
        let saved = app.settled_text(2014, |shown| shown.contains(&text(&target)));
        assert!(saved.contains(&text(&target)), "{saved}");
        assert!(!target.exists());
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn refuses_to_replace_without_overwrite() {
        let existing = temp_file("existing.txt");
        let _cleanup = TempFiles(vec![existing.clone()]);
        let Some(app) = FixtureApp::launch() else { return };
        let error = pick(&app, 2013, vec![text(&existing)], false).unwrap_err();
        assert_eq!(error.code, ErrorCode::NeedsUser);
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn replaces_with_overwrite() {
        let existing = temp_file("replace.txt");
        let _cleanup = TempFiles(vec![existing.clone()]);
        let Some(app) = FixtureApp::launch() else { return };
        pick(&app, 2013, vec![text(&existing)], true).unwrap();
        let saved = app.settled_text(2014, |shown| shown.contains(&text(&existing)));
        assert!(saved.contains(&text(&existing)), "{saved}");
    }
}
