use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use winpilot_core::Result;

use super::capture::AudioChunk;
use super::event::wait_for_any;
use super::feed::Catch;
use super::session::{Opened, Session};
use super::source::AudioSource;
use crate::com::ComApartment;

const WAKE_MS: u32 = 10;

pub(crate) struct ThreadInput {
    pub(crate) source: AudioSource,
    pub(crate) ready: Sender<Result<Opened>>,
    pub(crate) chunks: Sender<AudioChunk>,
    pub(crate) stop: Arc<AtomicBool>,
}

pub(crate) fn run(input: ThreadInput) -> Result<()> {
    let ThreadInput { source, ready, chunks, stop } = input;
    let apartment = match ComApartment::enter_mta_strict() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    let pumped = match Session::open(source) {
        Ok(session) if ready.send(Ok(session.opened())).is_ok() => pump(session, &chunks, &stop),
        Ok(_abandoned) => Ok(()),
        Err(error) => {
            let _ = ready.send(Err(error));
            Ok(())
        }
    };
    drop(apartment);
    pumped
}

fn pump(mut session: Session, chunks: &Sender<AudioChunk>, stop: &AtomicBool) -> Result<()> {
    let events = session.events();
    let mut first_frame = 0u64;
    let channels = session.opened().format.channel_count();
    loop {
        let stopping = stop.load(Ordering::Acquire);
        if !stopping {
            wait_for_any(&events, WAKE_MS)?;
        }
        let catch = if stopping { Catch::Flush } else { Catch::Idle };
        let samples = session.collect(catch)?;
        let frames = (samples.len() / channels) as u64;
        if frames > 0 && chunks.send(AudioChunk { first_frame, samples }).is_err() {
            return Ok(());
        }
        first_frame += frames;
        if stopping {
            return Ok(());
        }
    }
}
