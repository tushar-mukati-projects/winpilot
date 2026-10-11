mod indicator;
#[cfg(test)]
mod live_tests;
mod naming;
mod output;
mod redaction;
mod slot;
mod source;
mod start;

use deckhand_core::{
    Result,
    protocol::{RecordAction, RecordArgs, RecordReport},
};

pub use slot::RecordingSlot;

use crate::{call_scope::CallScope, engine::Engine};

pub fn run(engine: &Engine, args: &RecordArgs, call: &CallScope) -> Result<RecordReport> {
    args.validate()?;
    let report = match args.action {
        RecordAction::Start => start::start(engine, args, call),
        RecordAction::Stop => Ok(engine.recording.lock().stop()),
        RecordAction::Status => Ok(engine.recording.lock().status()),
    };
    indicator::show_recording_indicator(engine.overlay.as_ref(), engine.recording.lock().indicator_area());
    report
}
