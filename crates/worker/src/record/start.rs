use std::time::Duration;

use deckhand_capture::{Recorder, RecordingSpec, VideoSettings};
use deckhand_core::{
    Result,
    protocol::{RecordArgs, RecordReport},
    record_plan::{RecordPlan, record_plan},
};

use super::{output, redaction, slot::Active, source};
use crate::{call_scope::CallScope, engine::Engine};

const START_PATIENCE: Duration = Duration::from_secs(12);
const CLIPPED_NOTE: &str = "the region was clipped to the monitor it is on";

pub fn start(engine: &Engine, args: &RecordArgs, call: &CallScope) -> Result<RecordReport> {
    let plan = record_plan(args, &engine.config.record)?;
    let mut slot = engine.recording.lock();
    slot.ensure_free()?;
    engine.guard.check_running(call)?;
    let source = source::resolve(engine, args, call)?;
    let file = output::reserve(&engine.guard, &engine.config.record, args.path.as_deref(), plan.format)?;
    let spec = RecordingSpec {
        source: source.capture,
        path: file.path().to_path_buf(),
        video: video_settings(plan),
        redactor: source.redact.then(|| redaction::redactor(engine.config.guard.clone())),
        kill_switch: engine.guard.kill_switch_check(),
    };
    let recorder = Recorder::start(spec, call.bounded(START_PATIENCE))?;
    let path = file.keep().display().to_string();
    let note = recorder.started().clipped.then(|| CLIPPED_NOTE.to_string());
    Ok(slot.begin(Active { recorder, path, area: source.area, note }))
}

const fn video_settings(plan: RecordPlan) -> VideoSettings {
    VideoSettings {
        format: plan.format,
        fps: plan.fps,
        quality: plan.quality,
        max_seconds: plan.max_seconds,
        cursor: plan.cursor,
        max_width: plan.max_width,
    }
}
