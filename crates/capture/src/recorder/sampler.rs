use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, TrySendError};
use std::thread;
use std::time::{Duration, Instant};

use deckhand_core::Result;

use super::clock::{TickClock, total_ticks};
use super::encoder::{Encoded, EncoderInput, EncoderThread};
use super::layout::{MAX_VIDEO_HEIGHT, Size, output_size};
use super::outcome::{Counters, EndReason, Finished, Started};
use super::scaler::Scaler;
use super::source::Source;
use super::spec::{RecordingSource, RecordingSpec, Redactor, StopCheck, VideoSettings};
use super::video_writer::WriterSettings;
use crate::com::ComApartment;
use crate::dpi::make_thread_per_monitor_aware;

const REDACT_REFRESH: Duration = Duration::from_millis(500);
const STOP_POLL: Duration = Duration::from_millis(50);

pub(crate) struct SamplerInput {
    pub spec: RecordingSpec,
    pub stop: Arc<AtomicBool>,
    pub counters: Arc<Counters>,
    pub ready: Sender<Result<Started>>,
}

enum Ended {
    Because(EndReason),
    EncoderGone,
}

#[derive(Debug, Clone, Copy)]
struct Limit {
    ticks: u64,
    seconds: u32,
}

struct Prepared {
    source: Source,
    encoder: EncoderThread,
    output: Size,
    clipped: bool,
}

struct Sampling<'a> {
    source: Source,
    encoder: &'a EncoderThread,
    scaler: Scaler,
    output: Size,
    redactor: Option<Redactor>,
    unsent: Option<Vec<u8>>,
    source_unshown: bool,
    processed_at: Instant,
}

pub(crate) fn run(input: SamplerInput) -> Finished {
    let finished = match ComApartment::enter_mta() {
        Ok(apartment) => {
            let finished = record(input.spec, &input.stop, &input.counters, &input.ready);
            drop(apartment);
            finished
        }
        Err(error) => {
            let _ = input.ready.send(Err(error));
            Finished::failed_to_join()
        }
    };
    input.counters.finish();
    finished
}

fn record(spec: RecordingSpec, stop: &AtomicBool, counters: &Counters, ready: &Sender<Result<Started>>) -> Finished {
    let RecordingSpec { source, path, video, redactor, kill_switch } = spec;
    let Prepared { source, encoder, output, clipped } = match prepare(source, &path, video, stop) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Finished::failed_to_join();
        }
    };
    let _ = ready.send(Ok(Started { width: output.width, height: output.height, fps: video.fps, clipped }));
    let clock = TickClock::new(Instant::now(), video.fps);
    let limit = Limit { ticks: total_ticks(video.fps, video.max_seconds), seconds: video.max_seconds };
    let ended = Sampling::new(source, &encoder, output, redactor).run(&clock, limit, stop, &kill_switch, counters);
    let encoded = encoder.finish();
    finished(ended, encoded, counters, video.fps, &path)
}

fn prepare(source: RecordingSource, path: &Path, video: VideoSettings, stop: &AtomicBool) -> Result<Prepared> {
    make_thread_per_monitor_aware()?;
    let (mut source, clipped) = Source::open(source, video.cursor)?;
    source.wait_for_first_frame(stop)?;
    let output = output_size(source.size(), video.max_width, MAX_VIDEO_HEIGHT);
    let settings = WriterSettings::new(PathBuf::from(path), video.format, output, video.fps, video.quality);
    let encoder = EncoderThread::spawn(settings)?;
    Ok(Prepared { source, encoder, output, clipped })
}

fn finished(ended: Ended, encoded: Encoded, counters: &Counters, fps: u32, path: &Path) -> Finished {
    let progress = counters.progress(fps);
    let reason = match ended {
        Ended::Because(reason) => reason,
        Ended::EncoderGone => {
            EndReason::EncoderFailed(encoded.write_error.unwrap_or_else(|| "the encoder stopped".to_string()))
        }
    };
    Finished {
        frames: progress.frames,
        dropped: progress.dropped,
        seconds: progress.seconds,
        bytes: std::fs::metadata(path).map_or(0, |metadata| metadata.len()),
        reason,
        finalize_error: encoded.finalize_error,
    }
}

impl<'a> Sampling<'a> {
    fn new(source: Source, encoder: &'a EncoderThread, output: Size, redactor: Option<Redactor>) -> Self {
        Self {
            source,
            encoder,
            scaler: Scaler::new(),
            output,
            redactor,
            unsent: None,
            source_unshown: true,
            processed_at: Instant::now(),
        }
    }

    fn run(
        mut self,
        clock: &TickClock,
        limit: Limit,
        stop: &AtomicBool,
        kill_switch: &StopCheck,
        counters: &Counters,
    ) -> Ended {
        let mut tick = 0;
        loop {
            if let Some(ended) = self.interruption(clock, tick, limit, stop, kill_switch) {
                return ended;
            }
            let due = clock.due(Instant::now()).clamp(tick, limit.ticks.saturating_sub(1).max(tick));
            counters.dropped(due - tick);
            tick = due;
            if let Err(error) = self.refresh() {
                return Ended::Because(EndReason::CaptureFailed(error.message));
            }
            if !self.send(tick, counters) {
                return Ended::EncoderGone;
            }
            tick += 1;
            counters.reached(tick);
        }
    }

    fn interruption(
        &mut self,
        clock: &TickClock,
        tick: u64,
        limit: Limit,
        stop: &AtomicBool,
        kill_switch: &StopCheck,
    ) -> Option<Ended> {
        if tick >= limit.ticks {
            return Some(Ended::Because(EndReason::TimeLimit(limit.seconds)));
        }
        if wait_until(clock, tick, stop) {
            return Some(Ended::Because(EndReason::Stopped));
        }
        if kill_switch() {
            return Some(Ended::Because(EndReason::KillSwitch));
        }
        self.source.is_gone().then_some(Ended::Because(EndReason::WindowClosed))
    }

    fn refresh(&mut self) -> Result<()> {
        let fresh = self.source.grab()?;
        let redaction_due = self.redactor.is_some() && self.processed_at.elapsed() >= REDACT_REFRESH;
        if !fresh && !self.source_unshown && !redaction_due {
            return Ok(());
        }
        if let Some(redact) = self.redactor.as_mut() {
            redact(&mut self.source.frame);
        }
        let mut pixels = self.unsent.take().or_else(|| self.encoder.recycled.try_recv().ok()).unwrap_or_default();
        pixels.resize(self.output.bgra_len(), 0);
        let drawn = self.scaler.draw(&self.source.frame.bgra, self.source.size(), &mut pixels, self.output);
        self.unsent = Some(pixels);
        self.source_unshown = false;
        self.processed_at = Instant::now();
        drawn
    }

    fn send(&mut self, tick: u64, counters: &Counters) -> bool {
        let message = match self.unsent.take() {
            Some(pixels) => EncoderInput::Frame { tick, pixels },
            None => EncoderInput::Repeat { tick },
        };
        match self.encoder.input.try_send(message) {
            Ok(()) => counters.sent(),
            Err(TrySendError::Full(EncoderInput::Frame { pixels, .. })) => {
                self.unsent = Some(pixels);
                counters.dropped(1);
            }
            Err(TrySendError::Full(EncoderInput::Repeat { .. })) => counters.dropped(1),
            Err(TrySendError::Disconnected(_)) => return false,
        }
        true
    }
}

fn wait_until(clock: &TickClock, tick: u64, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Acquire) {
            return true;
        }
        let left = clock.wait_for(tick, Instant::now());
        if left.is_zero() {
            return false;
        }
        thread::sleep(left.min(STOP_POLL));
    }
}
