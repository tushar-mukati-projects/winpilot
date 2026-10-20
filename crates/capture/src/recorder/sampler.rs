use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, TrySendError};
use std::thread;
use std::time::{Duration, Instant};

use winpilot_core::Result;
use winpilot_win32::ComApartment;

use super::clock::{TickClock, total_ticks};
use super::encoder::{Encoded, EncoderInput, EncoderStart, EncoderThread};
use super::layout::{Size, output_size};
use super::outcome::{AudioTrack, Counters, EndReason, Finished, Started};
use super::output_file::OutputFile;
use super::scaler::Scaler;
use super::sound_start::{start_sound, without_sound};
use super::source::Source;
use super::spec::{RecordingSource, Redactor, StopCheck, VideoSettings};
use super::video_writer::WriterSettings;
use crate::dpi::make_thread_per_monitor_aware;

const STOP_POLL: Duration = Duration::from_millis(50);

pub(crate) struct SamplerInput {
    pub source: RecordingSource,
    pub output: Arc<OutputFile>,
    pub video: VideoSettings,
    pub redactor: Option<Redactor>,
    pub kill_switch: StopCheck,
    pub stop: Arc<AtomicBool>,
    pub deadline: Instant,
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
    size: Size,
    clipped: bool,
    audio: AudioTrack,
}

struct Sampling<'a> {
    source: Source,
    encoder: &'a EncoderThread,
    scaler: Scaler,
    size: Size,
    redactor: Option<Redactor>,
    unsent: Option<Vec<u8>>,
    source_unshown: bool,
}

struct Watch<'a> {
    stop: &'a AtomicBool,
    kill_switch: &'a StopCheck,
    counters: &'a Counters,
}

pub(crate) fn run(input: SamplerInput) -> Finished {
    let counters = Arc::clone(&input.counters);
    let finished = match ComApartment::enter_mta_strict() {
        Ok(apartment) => {
            let finished = record(input);
            drop(apartment);
            finished
        }
        Err(error) => {
            let _ = input.ready.send(Err(error));
            Finished::failed_to_join()
        }
    };
    counters.finish();
    finished
}

fn record(input: SamplerInput) -> Finished {
    let SamplerInput { source, output, video, redactor, kill_switch, stop, deadline, counters, ready } = input;
    let Prepared { source, encoder, size, clipped, audio } = match prepare(source, &output, video, &stop, deadline) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Finished::failed_to_join();
        }
    };
    let started = Started { width: size.width, height: size.height, fps: video.fps, clipped, audio: audio.clone() };
    let _ = ready.send(Ok(started));
    let clock = start_clock(&encoder, video.fps);
    let limit = Limit { ticks: total_ticks(video.fps, video.max_seconds), seconds: video.max_seconds };
    let watch = Watch { stop: &stop, kill_switch: &kill_switch, counters: &counters };
    let ended = Sampling::new(source, &encoder, size, redactor).run(&clock, limit, &watch);
    let encoded = encoder.finish(counters.ticks());
    finished(ended, encoded, audio, &counters, (video.fps, output.path()))
}

fn start_clock(encoder: &EncoderThread, fps: u32) -> TickClock {
    let start = Instant::now();
    let _ = encoder.input.send(EncoderInput::Begin { at: start });
    TickClock::new(start, fps)
}

fn prepare(
    source: RecordingSource,
    output: &Arc<OutputFile>,
    video: VideoSettings,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<Prepared> {
    make_thread_per_monitor_aware()?;
    let (mut source, clipped) = Source::open(source, video.cursor)?;
    source.wait_for_first_frame(stop, deadline)?;
    let size = output_size(source.size(), video.max_width);
    let settings = WriterSettings::new(output.path().to_path_buf(), video.format, size, video.fps, video.quality);
    let (capture, audio) = start_sound(video.audio);
    let start = EncoderStart { settings, audio: capture, output: Arc::clone(output), stop, deadline };
    let encoder = EncoderThread::spawn(start)?;
    let audio = encoder.audio_refusal.as_deref().map_or(audio, without_sound);
    Ok(Prepared { source, encoder, size, clipped, audio })
}

fn finished(ended: Ended, encoded: Encoded, audio: AudioTrack, counters: &Counters, file: (u32, &Path)) -> Finished {
    let (fps, path) = file;
    let progress = counters.progress(fps);
    let Encoded { write_error, finalize_error, audio_note } = encoded;
    let reason = match ended {
        Ended::Because(reason) => reason,
        Ended::EncoderGone => {
            EndReason::EncoderFailed(write_error.clone().unwrap_or_else(|| "the encoder stopped".to_string()))
        }
    };
    Finished {
        frames: progress.frames,
        dropped: progress.dropped,
        seconds: progress.seconds,
        bytes: std::fs::metadata(path).map_or(0, |metadata| metadata.len()),
        reason,
        write_error,
        finalize_error,
        audio: AudioTrack { source: audio.source, note: audio.note.or(audio_note) },
    }
}

impl<'a> Sampling<'a> {
    fn new(source: Source, encoder: &'a EncoderThread, size: Size, redactor: Option<Redactor>) -> Self {
        Self { source, encoder, scaler: Scaler::new(), size, redactor, unsent: None, source_unshown: true }
    }

    fn run(mut self, clock: &TickClock, limit: Limit, watch: &Watch<'_>) -> Ended {
        let mut tick = 0;
        loop {
            if let Some(ended) = self.interruption(clock, tick, limit, watch) {
                return ended;
            }
            let due = clock.due(Instant::now()).clamp(tick, limit.ticks.saturating_sub(1).max(tick));
            watch.counters.dropped(due - tick);
            tick = due;
            if let Err(error) = self.refresh() {
                return Ended::Because(EndReason::CaptureFailed(error.message));
            }
            if !self.send(tick, watch.counters) {
                return Ended::EncoderGone;
            }
            tick += 1;
            watch.counters.reached(tick);
        }
    }

    fn interruption(&mut self, clock: &TickClock, tick: u64, limit: Limit, watch: &Watch<'_>) -> Option<Ended> {
        if tick >= limit.ticks {
            return Some(Ended::Because(EndReason::TimeLimit(limit.seconds)));
        }
        if wait_until(clock, tick, watch.stop) {
            return Some(Ended::Because(EndReason::Stopped));
        }
        if (watch.kill_switch)() {
            return Some(Ended::Because(EndReason::KillSwitch));
        }
        self.source.ended().map(Ended::Because)
    }

    fn refresh(&mut self) -> Result<()> {
        let fresh = self.source.grab()?;
        let redaction_changed = self.redactor.as_mut().is_some_and(|redactor| redactor.refresh());
        if !fresh && !self.source_unshown && !redaction_changed {
            return Ok(());
        }
        if let Some(redactor) = self.redactor.as_ref() {
            redactor.cover(&mut self.source.frame);
        }
        let mut pixels = self.unsent.take().or_else(|| self.encoder.recycled.try_recv().ok()).unwrap_or_default();
        pixels.resize(self.size.bgra_len(), 0);
        let drawn = self.scaler.draw(&self.source.frame.bgra, self.source.size(), &mut pixels, self.size);
        self.unsent = Some(pixels);
        self.source_unshown = false;
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
            Err(TrySendError::Full(_)) => counters.dropped(1),
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
