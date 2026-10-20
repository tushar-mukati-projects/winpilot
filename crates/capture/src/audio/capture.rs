use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use winpilot_core::{Context, Error, Result};

use super::capture_thread::{ThreadInput, run};
use super::pcm::AudioFormat;
use super::session::Opened;
use super::source::AudioSource;
use crate::recorder::join_within;

const START_PATIENCE: Duration = Duration::from_secs(5);
const ABANDON_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AudioChunk {
    pub first_frame: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug)]
pub(crate) struct AudioEnd {
    pub chunks: Vec<AudioChunk>,
    pub error: Option<Error>,
}

pub(crate) struct AudioCapture {
    opened: Opened,
    chunks: Receiver<AudioChunk>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl AudioCapture {
    pub(crate) fn start(source: AudioSource) -> Result<Self> {
        let (ready, opened) = mpsc::channel();
        let (sender, chunks) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let input = ThreadInput { source, ready, chunks: sender, stop: Arc::clone(&stop) };
        let thread = thread::Builder::new()
            .name("winpilot-audio".to_string())
            .spawn(move || run(input))
            .context("starting the audio capture thread")?;
        match opened.recv_timeout(START_PATIENCE) {
            Ok(Ok(opened)) => Ok(Self { opened, chunks, stop, thread: Some(thread) }),
            Ok(Err(error)) => Err(abandon(thread, &stop, error)),
            Err(RecvTimeoutError::Timeout) => {
                let message = format!("the audio device did not start within {} s", START_PATIENCE.as_secs());
                Err(abandon(thread, &stop, Error::timeout(message)))
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(abandon(thread, &stop, Error::internal("the audio capture thread stopped while starting")))
            }
        }
    }

    pub(crate) const fn format(&self) -> AudioFormat {
        self.opened.format
    }

    pub(crate) const fn origin(&self) -> Instant {
        self.opened.origin
    }

    pub(crate) const fn chunks(&self) -> &Receiver<AudioChunk> {
        &self.chunks
    }

    pub(crate) fn stop(mut self) -> AudioEnd {
        self.finish()
    }

    fn finish(&mut self) -> AudioEnd {
        self.stop.store(true, Ordering::Release);
        let error = match self.thread.take().map(JoinHandle::join) {
            Some(Ok(Ok(()))) | None => None,
            Some(Ok(Err(error))) => Some(error),
            Some(Err(_)) => Some(Error::internal("the audio capture thread stopped unexpectedly")),
        };
        AudioEnd { chunks: self.chunks.try_iter().collect(), error }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.finish();
        }
    }
}

fn abandon(thread: JoinHandle<Result<()>>, stop: &AtomicBool, error: Error) -> Error {
    stop.store(true, Ordering::Release);
    join_within(thread, ABANDON_GRACE);
    error
}
