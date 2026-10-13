use std::thread;
use std::time::{Duration, Instant};

use winpilot_core::protocol::{RecordReport, RecordState};

use crate::engine::Engine;

const START_POLL: Duration = Duration::from_millis(20);
const CANCELLED_NOTE: &str = "the recording was stopped while it was starting; nothing was saved";

pub(crate) fn stop(engine: &Engine, patience: Duration) -> RecordReport {
    let cancelled_start = engine.recording.lock().cancel_start();
    if cancelled_start {
        wait_while_starting(engine, patience);
    }
    let report = engine.recording.lock().stop();
    if cancelled_start && report.state == RecordState::Idle {
        return RecordReport { note: Some(CANCELLED_NOTE.to_string()), ..report };
    }
    report
}

fn wait_while_starting(engine: &Engine, patience: Duration) {
    let started = Instant::now();
    while engine.recording.lock().is_starting() && started.elapsed() < patience {
        thread::sleep(START_POLL);
    }
}
