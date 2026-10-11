use std::{sync::mpsc::Sender, time::Duration};

use deckhand_core::{Point, Rect};

use super::chip_text::ChipText;

#[derive(Debug)]
pub(crate) enum OverlayCommand {
    Begin(Rect),
    End,
    Click(Point, u8),
    Glide(Point, Point, Duration),
    FocusField(Rect),
    Stopped,
    Resumed,
    Recording(Rect),
    RecordingStopped,
    Screenshot(Rect),
    Keystrokes(Rect, ChipText),
    Scan(Rect),
    Relabel(String),
    Conceal(Sender<()>),
    Unconceal,
}
