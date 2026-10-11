mod bitrate;
mod changed_area;
mod clock;
mod encoder;
mod gif_writer;
mod gpu;
mod layout;
mod media_foundation;
mod media_type;
mod monitor_area;
mod mp4;
mod mta_usage;
mod nv12;
mod outcome;
mod output_file;
mod sampler;
mod scaler;
mod source;
mod spec;
mod staging;
mod thread_join;
mod video_writer;
mod wgc;

#[cfg(test)]
mod live_tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use deckhand_core::{Context, Error, Result};

pub use outcome::{EndReason, Finished, Progress, Started};
pub use output_file::OutputFile;
pub use spec::{RecordingSource, RecordingSpec, Redact, Redactor, StopCheck, VideoSettings};

use mta_usage::keep_mta_for_process;
use outcome::Counters;
use sampler::SamplerInput;
use thread_join::join_within;

const ABANDON_GRACE: Duration = Duration::from_secs(1);

pub struct Recorder {
    started: Started,
    fps: u32,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    sampler: Option<JoinHandle<Finished>>,
    output: Arc<OutputFile>,
}

impl Recorder {
    pub fn start(spec: RecordingSpec, patience: Duration) -> Result<Self> {
        keep_mta_for_process()?;
        let RecordingSpec { source, output, video, redactor, kill_switch, stop } = spec;
        let (ready, started) = mpsc::channel();
        let mut recorder = Self {
            started: Started { width: 0, height: 0, fps: video.fps, clipped: false },
            fps: video.fps,
            stop,
            counters: Arc::new(Counters::default()),
            sampler: None,
            output: Arc::new(output),
        };
        let input = SamplerInput {
            source,
            output: Arc::clone(&recorder.output),
            video,
            redactor,
            kill_switch,
            stop: Arc::clone(&recorder.stop),
            deadline: Instant::now().checked_add(patience).unwrap_or_else(Instant::now),
            counters: Arc::clone(&recorder.counters),
            ready,
        };
        recorder.sampler = Some(spawn_sampler(input)?);
        recorder.await_start(&started, patience)?;
        Ok(recorder)
    }

    pub const fn started(&self) -> Started {
        self.started
    }

    pub fn progress(&self) -> Progress {
        self.counters.progress(self.fps)
    }

    pub fn has_ended(&self) -> bool {
        self.counters.is_finished()
    }

    pub fn stop(mut self) -> Finished {
        self.finish()
    }

    fn await_start(&mut self, started: &Receiver<Result<Started>>, patience: Duration) -> Result<()> {
        let error = match started.recv_timeout(patience) {
            Ok(Ok(started)) => {
                self.output.keep();
                self.started = started;
                return Ok(());
            }
            Ok(Err(error)) => error,
            Err(RecvTimeoutError::Timeout) => {
                Error::timeout(format!("the recording did not start within {:.0} s", patience.as_secs_f64()))
            }
            Err(RecvTimeoutError::Disconnected) => Error::internal("the recording thread stopped while starting"),
        };
        Err(self.abandon(error))
    }

    fn abandon(&mut self, error: Error) -> Error {
        self.stop.store(true, Ordering::Release);
        if let Some(sampler) = self.sampler.take() {
            join_within(sampler, ABANDON_GRACE);
        }
        error
    }

    fn finish(&mut self) -> Finished {
        self.stop.store(true, Ordering::Release);
        let Some(sampler) = self.sampler.take() else { return Finished::failed_to_join() };
        sampler.join().unwrap_or_else(|_| Finished::failed_to_join())
    }
}

fn spawn_sampler(input: SamplerInput) -> Result<JoinHandle<Finished>> {
    thread::Builder::new()
        .name("deckhand-record".to_string())
        .spawn(move || sampler::run(input))
        .context("starting the recording thread")
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.sampler.is_some() {
            self.finish();
        }
    }
}
