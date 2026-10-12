use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use deckhand_core::{Context, Error, ErrorCode, Result};

use super::audio_feed::{AudioFeed, TRACK_FORMAT};
use super::clock::audio_frames_at;
use super::output_file::OutputFile;
use super::thread_join::join_within;
use super::video_writer::{VideoWriter, WriterSettings};
use crate::audio::AudioCapture;
use crate::com::ComApartment;

const QUEUE_DEPTH: usize = 3;
const START_POLL: Duration = Duration::from_millis(20);
const ABANDON_GRACE: Duration = Duration::from_millis(250);

pub(crate) enum EncoderInput {
    Begin { at: Instant },
    Frame { tick: u64, pixels: Vec<u8> },
    Repeat { tick: u64 },
    End { tick: u64 },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Encoded {
    pub write_error: Option<String>,
    pub finalize_error: Option<String>,
    pub audio_note: Option<String>,
}

pub(crate) struct EncoderStart<'a> {
    pub settings: WriterSettings,
    pub audio: Option<AudioCapture>,
    pub output: Arc<OutputFile>,
    pub stop: &'a AtomicBool,
    pub deadline: Instant,
}

pub(crate) struct EncoderThread {
    pub input: SyncSender<EncoderInput>,
    pub recycled: Receiver<Vec<u8>>,
    pub audio_refusal: Option<String>,
    handle: JoinHandle<Encoded>,
}

struct Writing {
    writer: VideoWriter,
    audio: Option<AudioFeed>,
    fps: u32,
}

impl EncoderThread {
    pub(crate) fn spawn(start: EncoderStart<'_>) -> Result<Self> {
        let EncoderStart { mut settings, audio, output, stop, deadline } = start;
        settings.audio = audio.is_some();
        let (input, inbox) = mpsc::sync_channel(QUEUE_DEPTH);
        let (recycle, recycled) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("deckhand-encode".to_string())
            .spawn(move || {
                let encoded = run(&settings, audio, &inbox, &recycle, &ready);
                drop(output);
                encoded
            })
            .context("starting the video encoder thread")?;
        match wait_for_start(&started, stop, deadline) {
            Ok(audio_refusal) => Ok(Self { input, recycled, audio_refusal, handle }),
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
            ..Encoded::default()
        })
    }
}

fn wait_for_start(
    started: &Receiver<Result<Option<String>>>,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<Option<String>> {
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
    audio: Option<AudioCapture>,
    inbox: &Receiver<EncoderInput>,
    recycle: &Sender<Vec<u8>>,
    ready: &Sender<Result<Option<String>>>,
) -> Encoded {
    let apartment = match ComApartment::enter_mta_strict() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Encoded::default();
        }
    };
    let encoded = match VideoWriter::create(settings) {
        Ok(writer) => {
            let refusal = writer.audio_refusal();
            let audio = audio.filter(|_| refusal.is_none()).map(AudioFeed::new);
            let writing = Writing { writer, audio, fps: settings.fps };
            if ready.send(Ok(refusal)).is_ok() { encode(writing, inbox, recycle) } else { Encoded::default() }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            Encoded::default()
        }
    };
    drop(apartment);
    encoded
}

fn encode(mut writing: Writing, inbox: &Receiver<EncoderInput>, recycle: &Sender<Vec<u8>>) -> Encoded {
    let (end_tick, written) = write_all(&mut writing, inbox, recycle);
    let Writing { writer, audio, fps } = writing;
    let (audio_written, audio_note) =
        finish_audio(&writer, audio, audio_frames_at(end_tick, fps, TRACK_FORMAT.sample_rate));
    let finalized = writer.finish(end_tick);
    Encoded {
        write_error: written.and(audio_written).err().map(|error| error.message),
        finalize_error: finalized.err().map(|error| error.message),
        audio_note,
    }
}

fn finish_audio(writer: &VideoWriter, audio: Option<AudioFeed>, end_frame: u64) -> (Result<()>, Option<String>) {
    let Some(feed) = audio else { return (Ok(()), None) };
    let (samples, note) = feed.finish(end_frame);
    let written = samples.map_or(Ok(()), |samples| writer.write_audio(&samples));
    (written, note)
}

fn write_all(writing: &mut Writing, inbox: &Receiver<EncoderInput>, recycle: &Sender<Vec<u8>>) -> (u64, Result<()>) {
    let mut end_tick = 0;
    for input in inbox {
        let written = match input {
            EncoderInput::Begin { at } => {
                writing.begin_audio(at);
                Ok(())
            }
            EncoderInput::Frame { tick, pixels } => {
                end_tick = tick.saturating_add(1);
                let written = writing.writer.write(&pixels, tick);
                let _ = recycle.send(pixels);
                written.and_then(|()| writing.pump_audio(end_tick))
            }
            EncoderInput::Repeat { tick } => {
                end_tick = tick.saturating_add(1);
                writing.writer.repeat(tick).and_then(|()| writing.pump_audio(end_tick))
            }
            EncoderInput::End { tick } => return (end_tick.max(tick), Ok(())),
        };
        if written.is_err() {
            return (end_tick, written);
        }
    }
    (end_tick, Ok(()))
}

impl Writing {
    fn begin_audio(&mut self, at: Instant) {
        if let Some(feed) = self.audio.as_mut() {
            feed.begin(at);
        }
    }

    fn pump_audio(&mut self, video_end_tick: u64) -> Result<()> {
        let Some(feed) = self.audio.as_mut() else { return Ok(()) };
        let limit = audio_frames_at(video_end_tick, self.fps, TRACK_FORMAT.sample_rate);
        match feed.take(limit) {
            Some(samples) => self.writer.write_audio(&samples),
            None => Ok(()),
        }
    }
}
