use std::{sync::mpsc::Sender, time::Duration};

use deckhand_core::{Point, Rect};

use crate::keycaps::ChipText;

#[derive(Debug)]
pub(crate) enum OverlayCommand {
    Begin(Rect),
    End,
    Click(Point, u8),
    Trail(Point, Point),
    Glide(Point, Point, Duration),
    FocusField(Rect),
    Stopped,
    Resumed,
    Recording(Rect),
    RecordingStopped,
    Screenshot(Rect),
    Keystrokes(Rect, ChipText),
    Scan(Rect),
    Conceal(Sender<()>),
    Unconceal,
}
