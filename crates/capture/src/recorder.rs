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
mod nv12;
mod outcome;
mod sampler;
mod scaler;
mod source;
mod spec;
mod staging;
mod video_writer;
mod wgc;

#[cfg(test)]
mod live_tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use deckhand_core::{Context, Error, Result};

pub use outcome::{EndReason, Finished, Progress, Started};
pub use spec::{RecordingSource, RecordingSpec, Redactor, StopCheck, VideoSettings};

use outcome::Counters;
use sampler::SamplerInput;

pub struct Recorder {
    started: Started,
    fps: u32,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    sampler: Option<JoinHandle<Finished>>,
}

impl Recorder {
    pub fn start(spec: RecordingSpec, patience: Duration) -> Result<Self> {
        let fps = spec.video.fps;
        let stop = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(Counters::default());
        let (ready, started) = mpsc::channel();
        let input = SamplerInput { spec, stop: Arc::clone(&stop), counters: Arc::clone(&counters), ready };
        let sampler = thread::Builder::new()
            .name("deckhand-record".to_string())
            .spawn(move || sampler::run(input))
            .context("starting the recording thread")?;
        let outcome = started.recv_timeout(patience);
        let mut recorder = Self {
            started: Started { width: 0, height: 0, fps, clipped: false },
            fps,
            stop,
            counters,
            sampler: Some(sampler),
        };
        match outcome {
            Ok(Ok(started)) => {
                recorder.started = started;
                Ok(recorder)
            }
            Ok(Err(error)) => {
                recorder.finish();
                Err(error)
            }
            Err(_) => {
                recorder.finish();
                Err(Error::timeout(format!("the recording did not start within {:.0} s", patience.as_secs_f64())))
            }
        }
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

    fn finish(&mut self) -> Finished {
        self.stop.store(true, Ordering::Release);
        let Some(sampler) = self.sampler.take() else { return Finished::failed_to_join() };
        sampler.join().unwrap_or_else(|_| Finished::failed_to_join())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.sampler.is_some() {
            self.finish();
        }
    }
}
