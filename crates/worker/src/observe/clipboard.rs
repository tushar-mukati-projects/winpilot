use deckhand_core::{Result, protocol::ClipboardReport};
use deckhand_desktop::clipboard;

pub fn read() -> Result<ClipboardReport> {
    Ok(ClipboardReport { text: clipboard::get_text()? })
}
