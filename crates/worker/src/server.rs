use std::{
    io::{self, BufReader, BufWriter, Stdout},
    sync::Arc,
    thread,
    time::Duration,
};

use deckhand_core::{
    Context, Error, Result,
    ipc::{FromWorker, ToWorker, read_frame, write_frame},
    protocol::{Call, Reply},
};
use parking_lot::Mutex;

use crate::{com::ComGuard, engine::Engine, in_flight::InFlight};

const SHUTDOWN_PATIENCE: Duration = Duration::from_secs(1);

type Outbox = Arc<Mutex<BufWriter<Stdout>>>;

struct Request {
    id: u64,
    call: Call,
    budget: Duration,
}

pub fn run(build: &str) -> Result<()> {
    deckhand_desktop::enable_dpi_awareness()?;
    let _com = ComGuard::multithreaded();
    let mut inbox = BufReader::new(io::stdin().lock());
    let outbox: Outbox = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    let (config, epoch, agent) = match read_frame(&mut inbox).context("reading the init message")? {
        Some(ToWorker::Init { config, epoch, agent }) => (*config, epoch, agent),
        _ => return Err(Error::invalid("the first message to the worker must be init")),
    };
    let engine = Arc::new(Engine::start(config, epoch, &agent)?);
    send(&outbox, &FromWorker::Ready { version: deckhand_core::VERSION.to_string(), build: build.to_string() })?;
    let in_flight = Arc::new(InFlight::default());
    let served = serve(&mut inbox, &engine, &outbox, &in_flight);
    engine.begin_shut_down();
    let unfinished = in_flight.cancel_all_and_wait(SHUTDOWN_PATIENCE);
    if unfinished > 0 {
        eprintln!("deckhand worker: {unfinished} call(s) were still running at shutdown");
    }
    engine.shut_down();
    served
}

fn serve(inbox: &mut impl io::Read, engine: &Arc<Engine>, outbox: &Outbox, in_flight: &Arc<InFlight>) -> Result<()> {
    while let Some(message) = read_frame::<_, ToWorker>(inbox).context("reading a request")? {
        match message {
            ToWorker::Call { id, call, deadline_ms } => {
                let request = Request { id, call, budget: Duration::from_millis(deadline_ms) };
                start_call(request, engine, outbox, in_flight);
            }
            ToWorker::Cancel { id } => in_flight.cancel(id),
            ToWorker::Agent { name } => engine.rename_agent(&name),
            ToWorker::Init { .. } => eprintln!("deckhand worker: ignoring a repeated init message"),
        }
    }
    Ok(())
}

fn start_call(request: Request, engine: &Arc<Engine>, outbox: &Outbox, in_flight: &Arc<InFlight>) {
    let Request { id, call, budget } = request;
    let cancel = in_flight.register(id);
    let engine = Arc::clone(engine);
    let thread_outbox = Arc::clone(outbox);
    let thread_in_flight = Arc::clone(in_flight);
    let spawned = thread::Builder::new().name(format!("deckhand-call-{id}")).spawn(move || {
        let _com = ComGuard::multithreaded();
        let result = engine.handle(&call, &cancel, budget);
        thread_in_flight.finished(id);
        reply(&thread_outbox, id, result);
    });
    match spawned {
        Ok(thread) => in_flight.track(thread),
        Err(error) => {
            in_flight.finished(id);
            reply(outbox, id, Err(Error::internal(format!("could not start a thread for the request: {error}"))));
        }
    }
}

fn reply(outbox: &Outbox, id: u64, result: Result<Reply>) {
    let Err(error) = send(outbox, &FromWorker::Reply { id, result: Box::new(result) }) else { return };
    eprintln!("deckhand worker: could not send a reply: {error}");
    let unsendable = Error::internal(format!("the reply could not be sent to the server: {}", error.message));
    if let Err(again) = send(outbox, &FromWorker::Reply { id, result: Box::new(Err(unsendable)) }) {
        eprintln!("deckhand worker: could not send the failure either: {again}");
    }
}

fn send(outbox: &Outbox, message: &FromWorker) -> Result<()> {
    write_frame(&mut *outbox.lock(), message).context("writing to the server")
}
