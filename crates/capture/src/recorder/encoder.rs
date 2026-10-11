use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use deckhand_core::{Context, Error, ErrorCode, Result};

use super::output_file::OutputFile;
use super::thread_join::join_within;
use super::video_writer::{VideoWriter, WriterSettings};
use crate::com::ComApartment;

const QUEUE_DEPTH: usize = 3;
const START_POLL: Duration = Duration::from_millis(20);
const ABANDON_GRACE: Duration = Duration::from_millis(250);

pub(crate) enum EncoderInput {
    Frame { tick: u64, pixels: Vec<u8> },
    Repeat { tick: u64 },
    End { tick: u64 },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Encoded {
    pub write_error: Option<String>,
    pub finalize_error: Option<String>,
}

pub(crate) struct EncoderStart<'a> {
    pub settings: WriterSettings,
    pub output: Arc<OutputFile>,
    pub stop: &'a AtomicBool,
    pub deadline: Instant,
}

pub(crate) struct EncoderThread {
    pub input: SyncSender<EncoderInput>,
    pub recycled: Receiver<Vec<u8>>,
    handle: JoinHandle<Encoded>,
}

impl EncoderThread {
    pub(crate) fn spawn(start: EncoderStart<'_>) -> Result<Self> {
        let EncoderStart { settings, output, stop, deadline } = start;
        let (input, inbox) = mpsc::sync_channel(QUEUE_DEPTH);
        let (recycle, recycled) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("deckhand-encode".to_string())
            .spawn(move || {
                let encoded = run(&settings, &inbox, &recycle, &ready);
                drop(output);
                encoded
            })
            .context("starting the video encoder thread")?;
        match wait_for_start(&started, stop, deadline) {
            Ok(()) => Ok(Self { input, recycled, handle }),
            Err(error) => {
                drop(started);
                drop(input);
                join_within(handle, ABANDON_GRACE);
                Err(error)
            }
        }
    }

    pub(crate) fn finish(self, end_tick: u64) -> Encoded {
        let _ = self.input.send(EncoderInput::End { tick: end_tick });
        drop(self.input);
        self.handle.join().unwrap_or_else(|_| Encoded {
            write_error: Some("the encoder thread stopped unexpectedly".to_string()),
            finalize_error: None,
        })
    }
}

fn wait_for_start(started: &Receiver<Result<()>>, stop: &AtomicBool, deadline: Instant) -> Result<()> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(Error::new(ErrorCode::Cancelled, "the recording was stopped while starting"));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::timeout("the video encoder did not start in time"));
        }
        match started.recv_timeout(left.min(START_POLL)) {
            Ok(outcome) => return outcome,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(Error::internal("the video encoder stopped while starting"));
            }
        }
    }
}

fn run(
    settings: &WriterSettings,
    inbox: &Receiver<EncoderInput>,
    recycle: &Sender<Vec<u8>>,
    ready: &Sender<Result<()>>,
) -> Encoded {
    let apartment = match ComApartment::enter_mta() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Encoded::default();
        }
    };
    let encoded = match VideoWriter::create(settings) {
        Ok(writer) if ready.send(Ok(())).is_ok() => encode(writer, inbox, recycle),
        Ok(_abandoned) => Encoded::default(),
        Err(error) => {
            let _ = ready.send(Err(error));
            Encoded::default()
        }
    };
    drop(apartment);
    encoded
}

fn encode(mut writer: VideoWriter, inbox: &Receiver<EncoderInput>, recycle: &Sender<Vec<u8>>) -> Encoded {
    let (end_tick, written) = write_all(&mut writer, inbox, recycle);
    let finalized = writer.finish(end_tick);
    Encoded {
        write_error: written.err().map(|error| error.message),
        finalize_error: finalized.err().map(|error| error.message),
    }
}

fn write_all(writer: &mut VideoWriter, inbox: &Receiver<EncoderInput>, recycle: &Sender<Vec<u8>>) -> (u64, Result<()>) {
    let mut end_tick = 0;
    for input in inbox {
        let written = match input {
            EncoderInput::Frame { tick, pixels } => {
                end_tick = tick.saturating_add(1);
                let written = writer.write(&pixels, tick);
                let _ = recycle.send(pixels);
                written
            }
            EncoderInput::Repeat { tick } => {
                end_tick = tick.saturating_add(1);
                writer.repeat(tick)
            }
            EncoderInput::End { tick } => return (end_tick.max(tick), Ok(())),
        };
        if written.is_err() {
            return (end_tick, written);
        }
    }
    (end_tick, Ok(()))
}
