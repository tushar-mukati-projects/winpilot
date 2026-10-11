use deckhand_core::{Point, Rect, Result};

pub struct Overlay {
    _private: (),
}

impl Overlay {
    pub fn spawn() -> Result<Self> {
        todo!()
    }

    pub fn show_target(&self, rect: Rect) {
        todo!("{rect:?}")
    }

    pub fn flash_point(&self, point: Point) {
        todo!("{point:?}")
    }

    pub fn hide(&self) {
        todo!()
    }
}
