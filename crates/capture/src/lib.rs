use deckhand_core::{Point, Rect, Result, WindowRef, protocol::ImageFormat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub origin: Point,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Frame {
    pub fn rect(&self) -> Rect {
        Rect::new(self.origin.x, self.origin.y, self.width as i32, self.height as i32)
    }

    pub fn crop(&self, rect: Rect) -> Option<Self> {
        todo!("{rect:?}")
    }
}

#[derive(Clone)]
pub struct Capturer {
    _private: (),
}

impl Capturer {
    pub fn spawn() -> Result<Self> {
        todo!()
    }

    pub fn monitor_rects(&self) -> Result<Vec<Rect>> {
        todo!()
    }

    pub fn screen(&self, rect: Rect) -> Result<Frame> {
        todo!("{rect:?}")
    }

    pub fn monitor(&self, index: usize) -> Result<Frame> {
        todo!("{index}")
    }

    pub fn window(&self, hwnd: WindowRef) -> Result<Frame> {
        todo!("{hwnd}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub max_width: u32,
    pub max_height: u32,
    pub format: ImageFormat,
    pub quality: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub rect: Rect,
    pub label: String,
}

pub fn encode(frame: &Frame, options: &EncodeOptions, marks: &[Mark]) -> Result<Encoded> {
    todo!("{frame:?} {options:?} {marks:?}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrLine {
    pub text: String,
    pub rect: Rect,
}

pub fn ocr(frame: &Frame) -> Result<Vec<OcrLine>> {
    todo!("{}", frame.width)
}
