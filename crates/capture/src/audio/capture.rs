use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use deckhand_core::{Context, Error, Result};

use super::capture_thread::{ThreadInput, run};
use super::pcm::AudioFormat;
use super::session::Opened;
use super::source::AudioSource;
use crate::recorder::join_within;

const START_PATIENCE: Duration = Duration::from_secs(5);
const ABANDON_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    pub first_frame: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug)]
pub struct AudioEnd {
    pub chunks: Vec<AudioChunk>,
    pub error: Option<Error>,
}

pub struct AudioCapture {
    source: AudioSource,
    opened: Opened,
    chunks: Receiver<AudioChunk>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl AudioCapture {
    pub fn start(source: AudioSource) -> Result<Self> {
        let (ready, opened) = mpsc::channel();
        let (sender, chunks) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let input = ThreadInput { source, ready, chunks: sender, stop: Arc::clone(&stop) };
        let thread = thread::Builder::new()
            .name("deckhand-audio".to_string())
            .spawn(move || run(input))
            .context("starting the audio capture thread")?;
        match opened.recv_timeout(START_PATIENCE) {
            Ok(Ok(opened)) => Ok(Self { source, opened, chunks, stop, thread: Some(thread) }),
            Ok(Err(error)) => Err(abandon(thread, &stop, error)),
            Err(RecvTimeoutError::Timeout) => {
                Err(abandon(thread, &stop, Error::timeout("the audio device did not start within 5 s")))
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(abandon(thread, &stop, Error::internal("the audio capture thread stopped while starting")))
            }
        }
    }

    pub const fn source(&self) -> AudioSource {
        self.source
    }

    pub const fn format(&self) -> AudioFormat {
        self.opened.format
    }

    pub const fn origin(&self) -> Instant {
        self.opened.origin
    }

    pub const fn chunks(&self) -> &Receiver<AudioChunk> {
        &self.chunks
    }

    pub fn stop(mut self) -> AudioEnd {
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
