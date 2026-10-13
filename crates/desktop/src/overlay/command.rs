use std::{sync::mpsc::Sender, time::Duration};

use deckhand_core::{Point, Rect};

use super::{activity::Activity, chip_text::ChipText, driving::Driving};

#[derive(Debug)]
pub(crate) enum OverlayCommand {
    Begin(Rect, Driving),
    End,
    Click(Point, u8),
    Ripple(Point, u8),
    Glide(Point, Point, Duration),
    FocusField(Rect),
    Activity(Activity),
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
