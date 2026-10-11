use std::mem::size_of;

use deckhand_core::{Error, ErrorCode, Result};
use windows::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, GetLastError, WIN32_ERROR},
    UI::Input::KeyboardAndMouse::{INPUT, SendInput},
};

use crate::{batch::Batch, call_split::split_into_calls, event::InputEvent};

const BLOCKED_BY_WINDOWS: &str =
    "input was blocked by Windows (the target window is elevated or the secure desktop is active)";

pub fn send(events: &[InputEvent]) -> Result<()> {
    for call in split_into_calls(events) {
        send_call(call)?;
    }
    Ok(())
}

pub fn send_batch(batch: &Batch) -> Result<()> {
    let outcome = send(&batch.events);
    if outcome.is_err() && !batch.release_on_failure.is_empty() {
        let _ = send(&batch.release_on_failure);
    }
    outcome
}

fn send_call(events: &[InputEvent]) -> Result<()> {
    let inputs: Vec<INPUT> = events.iter().map(|event| event.to_win32()).collect();
    let inserted = insert_inputs(&inputs);
    if inserted as usize == inputs.len() {
        return Ok(());
    }
    Err(blocked_input_error(last_error(), inserted, inputs.len()))
}

fn insert_inputs(inputs: &[INPUT]) -> u32 {
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) }
}

fn last_error() -> WIN32_ERROR {
    unsafe { GetLastError() }
}

fn blocked_input_error(error: WIN32_ERROR, inserted: u32, expected: usize) -> Error {
    if error == ERROR_ACCESS_DENIED {
        return Error::new(ErrorCode::ElevatedTarget, BLOCKED_BY_WINDOWS);
    }
    Error::internal(format!("SendInput inserted {inserted} of {expected} input events (Win32 error {})", error.0))
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::ERROR_INVALID_PARAMETER;

    use super::*;

    #[test]
    fn access_denied_means_elevated_target() {
        let error = blocked_input_error(ERROR_ACCESS_DENIED, 0, 4);
        assert_eq!(error.code, ErrorCode::ElevatedTarget);
        assert_eq!(error.message, BLOCKED_BY_WINDOWS);
    }

    #[test]
    fn other_failures_are_internal() {
        let error = blocked_input_error(ERROR_INVALID_PARAMETER, 1, 4);
        assert_eq!(error.code, ErrorCode::Internal);
        assert!(error.message.contains("1 of 4"));
    }

    #[test]
    fn sending_nothing_makes_no_calls() {
        assert!(send(&[]).is_ok());
    }
}
