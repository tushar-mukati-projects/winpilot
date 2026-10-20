use windows::Win32::UI::Input::KeyboardAndMouse::INPUT;
use winpilot_core::Result;
use winpilot_win32::send_input;

use crate::{batch::Batch, call_split::split_into_calls, event::InputEvent};

pub(crate) fn send(events: &[InputEvent]) -> Result<()> {
    for call in split_into_calls(events) {
        send_call(call)?;
    }
    Ok(())
}

pub(crate) fn send_batch(batch: &Batch) -> Result<()> {
    let outcome = send(&batch.events);
    if outcome.is_err() && !batch.release_on_failure.is_empty() {
        let _ = send(&batch.release_on_failure);
    }
    outcome
}

fn send_call(events: &[InputEvent]) -> Result<()> {
    let inputs: Vec<INPUT> = events.iter().map(|event| event.to_win32()).collect();
    send_input(&inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sending_nothing_makes_no_calls() {
        assert!(send(&[]).is_ok());
    }
}
