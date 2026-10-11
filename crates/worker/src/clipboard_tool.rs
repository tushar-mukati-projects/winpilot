use deckhand_core::{
    Error, Result,
    protocol::{ClipboardAction, ClipboardArgs, ClipboardReport},
};
use deckhand_desktop::clipboard;

use crate::{call_scope::CallScope, engine::Engine};

pub fn run(engine: &Engine, args: &ClipboardArgs, call: &CallScope) -> Result<ClipboardReport> {
    match args.action {
        ClipboardAction::Get => Ok(ClipboardReport { text: clipboard::get_text()?, changed: false }),
        ClipboardAction::Set => {
            let text = args.text.as_deref().ok_or_else(|| Error::invalid("clipboard set needs \"text\""))?;
            let _actor = engine.guard.acquire_actor(call)?;
            engine.guard.check_running(call)?;
            clipboard::set_text(text)?;
            Ok(ClipboardReport { text: Some(text.to_string()), changed: true })
        }
    }
}
