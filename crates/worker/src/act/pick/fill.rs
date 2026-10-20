use std::time::Duration;

use winpilot_core::{ElementRef, Error, ErrorCode, Result};

use super::{
    dialog::{self, FileDialog},
    folder::{Destination, folder_ready, listed_files},
    selection::Selection,
};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const FOLDER_WAIT: Duration = Duration::from_secs(5);

pub(super) fn fill(engine: &Engine, dialog: &FileDialog, selection: &Selection, call: &CallScope) -> Result<()> {
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

pub(super) fn press(engine: &Engine, button: ElementRef) -> Result<()> {
    let pressed = engine.uia.run(move |uia| match uia.invoke(button) {
        Err(error) if error.code == ErrorCode::Unsupported => uia.default_action(button),
        result => result,
    });
    match pressed {
        Err(error) if error.code == ErrorCode::TargetNotResponding => Ok(()),
        result => result,
    }
}
