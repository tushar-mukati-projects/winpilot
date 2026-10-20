use std::time::Duration;

use winpilot_core::{ElementRef, Error, ErrorCode, Result, WindowInfo, WindowRef};

use super::{
    dialog::FileDialog,
    dialog_windows,
    fill::press,
    message_box::{self, Prompt},
    selection::Requested,
};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const CLOSE_WAIT: Duration = Duration::from_secs(5);

enum Outcome {
    Closed,
    Prompt(WindowInfo),
}

pub(super) fn confirm(
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
                format!("the file dialog asked: {text}; WinPilot declined, so nothing was picked"),
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
        if !winpilot_win32::window_exists(hwnd) {
            return Ok(Some(Outcome::Closed));
        }
        Ok(new_prompt(engine, hwnd, answered)?.map(Outcome::Prompt))
    })
}

fn new_prompt(engine: &Engine, owner: WindowRef, answered: Option<WindowRef>) -> Result<Option<WindowInfo>> {
    let mut prompts = dialog_windows(engine)?.into_iter().filter(|window| Some(window.hwnd) != answered);
    Ok(prompts.find(|window| winpilot_win32::owner_of(window.hwnd) == Some(owner)))
}

fn still_open(dialog: &FileDialog) -> Error {
    Error::timeout(format!("the {} dialog is still open after confirming", dialog.kind.name()))
}
