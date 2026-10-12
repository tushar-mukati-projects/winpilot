mod indicator;
#[cfg(test)]
mod live_tests;
mod naming;
mod output;
mod redaction;
mod slot;
mod source;
mod start;
mod stop;
mod window_watch;

use std::time::Duration;

use deckhand_core::{
    Result,
    protocol::{RecordAction, RecordReport, RecordRequest},
};

pub(crate) use slot::RecordingSlot;

use crate::{call_scope::CallScope, engine::Engine};

const SHUTDOWN_START_WAIT: Duration = Duration::from_secs(1);

pub(crate) fn run(engine: &Engine, args: &RecordRequest, call: &CallScope) -> Result<RecordReport> {
    let report = match args.action {
        RecordAction::Start => start::start(engine, args, call),
        RecordAction::Stop => Ok(stop::stop(engine, call.remaining())),
        RecordAction::Status => Ok(engine.recording.lock().status()),
    };
    indicator::show_recording_indicator(engine.overlay.as_deref(), engine.recording.lock().indicator_area());
    report
}

pub(crate) fn shut_down(engine: &Engine) {
    stop::stop(engine, SHUTDOWN_START_WAIT);
    indicator::show_recording_indicator(engine.overlay.as_deref(), None);
}
