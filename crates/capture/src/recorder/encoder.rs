use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use deckhand_core::{Context, Error, Result};

use super::video_writer::{VideoWriter, WriterSettings};
use crate::com::ComApartment;

const QUEUE_DEPTH: usize = 3;
const START_PATIENCE: Duration = Duration::from_secs(10);

pub(crate) enum EncoderInput {
    Frame { tick: u64, pixels: Vec<u8> },
    Repeat { tick: u64 },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Encoded {
    pub write_error: Option<String>,
    pub finalize_error: Option<String>,
}

pub(crate) struct EncoderThread {
    pub input: SyncSender<EncoderInput>,
    pub recycled: Receiver<Vec<u8>>,
    handle: JoinHandle<Encoded>,
}

impl EncoderThread {
    pub(crate) fn spawn(settings: WriterSettings) -> Result<Self> {
        let (input, inbox) = mpsc::sync_channel(QUEUE_DEPTH);
        let (recycle, recycled) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("deckhand-encode".to_string())
            .spawn(move || run(&settings, &inbox, &recycle, &ready))
            .context("starting the video encoder thread")?;
        match started.recv_timeout(START_PATIENCE) {
            Ok(Ok(())) => Ok(Self { input, recycled, handle }),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => Err(Error::timeout("the video encoder did not start within 10 s")),
        }
    }

    pub(crate) fn finish(self) -> Encoded {
        drop(self.input);
        self.handle.join().unwrap_or_else(|_| Encoded {
            write_error: Some("the encoder thread stopped unexpectedly".to_string()),
            finalize_error: None,
        })
    }
}

fn run(
    settings: &WriterSettings,
    inbox: &Receiver<EncoderInput>,
    recycle: &Sender<Vec<u8>>,
    ready: &Sender<Result<()>>,
) -> Encoded {
    let opened = ComApartment::enter_mta().and_then(|apartment| Ok((apartment, VideoWriter::create(settings)?)));
    let (apartment, mut writer) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Encoded::default();
        }
    };
    let _ = ready.send(Ok(()));
    let (end_tick, written) = write_all(&mut writer, inbox, recycle);
    let finalized = writer.finish(end_tick);
    drop(apartment);
    Encoded {
        write_error: written.err().map(|error| error.message),
        finalize_error: finalized.err().map(|error| error.message),
    }
}

fn write_all(writer: &mut VideoWriter, inbox: &Receiver<EncoderInput>, recycle: &Sender<Vec<u8>>) -> (u64, Result<()>) {
    let mut shown: Option<Vec<u8>> = None;
    let mut end_tick = 0;
    for input in inbox {
        let written = match input {
            EncoderInput::Frame { tick, pixels } => {
                end_tick = tick.saturating_add(1);
                let written = writer.write(&pixels, tick);
                if let Some(previous) = shown.replace(pixels) {
                    let _ = recycle.send(previous);
                }
                written
            }
            EncoderInput::Repeat { tick } => {
                end_tick = tick.saturating_add(1);
                shown.as_ref().map_or(Ok(()), |pixels| writer.repeat(pixels, tick))
            }
        };
        if written.is_err() {
            return (end_tick, written);
        }
    }
    (end_tick, Ok(()))
}
