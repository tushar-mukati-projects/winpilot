use std::{
    collections::HashMap,
    io::{self, BufReader, BufWriter, Stdout},
    sync::Arc,
    thread,
};

use deckhand_core::{
    Context, Error, Result,
    ipc::{FromWorker, ToWorker, read_frame, write_frame},
    protocol::Call,
};
use parking_lot::Mutex;

use crate::{cancel::Cancel, com::ComGuard, engine::Engine};

type Outbox = Arc<Mutex<BufWriter<Stdout>>>;
type Pending = Arc<Mutex<HashMap<u64, Cancel>>>;

pub fn run() -> Result<()> {
    deckhand_desktop::enable_dpi_awareness()?;
    let _com = ComGuard::multithreaded();
    let mut inbox = BufReader::new(io::stdin().lock());
    let outbox: Outbox = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    let (config, epoch, agent) = match read_frame(&mut inbox).context("reading the init message")? {
        Some(ToWorker::Init { config, epoch, agent }) => (*config, epoch, agent),
        _ => return Err(Error::invalid("the first message to the worker must be init")),
    };
    let engine = Arc::new(Engine::start(config, epoch, &agent)?);
    send(&outbox, &FromWorker::Ready { version: deckhand_core::VERSION.to_string() })?;
    let served = serve(&mut inbox, &engine, &outbox);
    engine.shut_down();
    served
}

fn serve(inbox: &mut impl io::Read, engine: &Arc<Engine>, outbox: &Outbox) -> Result<()> {
    let pending: Pending = Arc::default();
    while let Some(message) = read_frame::<_, ToWorker>(inbox).context("reading a request")? {
        match message {
            ToWorker::Call { id, call } => start_call(id, call, engine, outbox, &pending),
            ToWorker::Cancel { id } => {
                if let Some(cancel) = pending.lock().get(&id) {
                    cancel.cancel();
                }
            }
            ToWorker::Init { .. } => eprintln!("deckhand worker: ignoring a repeated init message"),
        }
    }
    Ok(())
}

fn start_call(id: u64, call: Call, engine: &Arc<Engine>, outbox: &Outbox, pending: &Pending) {
    let cancel = Cancel::default();
    pending.lock().insert(id, cancel.clone());
    let engine = Arc::clone(engine);
    let thread_outbox = Arc::clone(outbox);
    let thread_pending = Arc::clone(pending);
    let spawned = thread::Builder::new().name(format!("deckhand-call-{id}")).spawn(move || {
        let _com = ComGuard::multithreaded();
        let result = engine.handle(call, &cancel);
        thread_pending.lock().remove(&id);
        reply(&thread_outbox, id, result);
    });
    if let Err(error) = spawned {
        pending.lock().remove(&id);
        reply(outbox, id, Err(Error::internal(format!("could not start a thread for the request: {error}"))));
    }
}

fn reply(outbox: &Outbox, id: u64, result: Result<deckhand_core::protocol::Reply>) {
    if let Err(error) = send(outbox, &FromWorker::Reply { id, result: Box::new(result) }) {
        eprintln!("deckhand worker: could not send a reply: {error}");
    }
}

fn send(outbox: &Outbox, message: &FromWorker) -> Result<()> {
    write_frame(&mut *outbox.lock(), message).context("writing to the server")
}
