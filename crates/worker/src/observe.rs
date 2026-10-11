mod apps;
mod clipboard;
mod elements;
mod monitors;
pub mod record;
mod screenshot;
mod search;
mod status;
mod text;
mod windows;

use deckhand_core::{
    Result,
    protocol::{ObserveArgs, ObserveRequest, Reply},
};

pub use self::{elements::observe_window, screenshot::of_window};
use crate::{call_scope::CallScope, engine::Engine};

pub fn run(engine: &Engine, args: &ObserveArgs, call: &CallScope) -> Result<Reply> {
    match args.request()? {
        ObserveRequest::Elements(request) => elements::run(engine, &request).map(Reply::Observation),
        ObserveRequest::Search(request) => search::run(engine, &request, call).map(Reply::Search),
        ObserveRequest::Text(request) => text::run(engine, &request, call).map(Reply::Text),
        ObserveRequest::Screenshot(request) => screenshot::run(engine, &request, call).map(Reply::Screenshot),
        ObserveRequest::Record(request) => record::run(engine, &request, call).map(Reply::Record),
        ObserveRequest::Windows => windows::list(engine).map(Reply::Windows),
        ObserveRequest::Monitors => monitors::list().map(Reply::Monitors),
        ObserveRequest::Apps(request) => apps::list(engine, &request).map(Reply::Apps),
        ObserveRequest::Clipboard(request) => clipboard::read(engine, request).map(Reply::Clipboard),
        ObserveRequest::Status => Ok(Reply::Status(status::report(engine))),
    }
}
