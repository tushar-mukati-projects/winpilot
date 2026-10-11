use std::time::Duration;

use deckhand_core::{Element, ElementRef, Point, Result, Snapshot, WindowInfo, WindowRef, protocol::Direction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiaOptions {
    pub connection_timeout: Duration,
    pub transaction_timeout: Duration,
}

impl Default for UiaOptions {
    fn default() -> Self {
        Self { connection_timeout: Duration::from_secs(2), transaction_timeout: Duration::from_secs(3) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOptions {
    pub max_elements: usize,
    pub include_offscreen: bool,
    pub budget: Duration,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self { max_elements: 3000, include_offscreen: false, budget: Duration::from_millis(2500) }
    }
}

pub struct Uia {
    _private: (),
}

impl Uia {
    pub fn snapshot(&mut self, window: &WindowInfo, options: &SnapshotOptions) -> Result<Snapshot> {
        todo!("{window:?} {options:?}")
    }

    pub fn element(&mut self, id: ElementRef) -> Result<Element> {
        todo!("{id}")
    }

    pub fn element_at(&mut self, point: Point) -> Result<Element> {
        todo!("{point:?}")
    }

    pub fn focused(&mut self) -> Result<Option<Element>> {
        todo!()
    }

    pub fn contains(&mut self, ancestor: ElementRef, descendant: ElementRef) -> Result<bool> {
        todo!("{ancestor} {descendant}")
    }

    pub fn window_of(&mut self, id: ElementRef) -> Result<WindowRef> {
        todo!("{id}")
    }

    pub fn invoke(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn toggle(&mut self, id: ElementRef) -> Result<bool> {
        todo!("{id}")
    }

    pub fn select(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn select_option(&mut self, id: ElementRef, option: &str) -> Result<()> {
        todo!("{id} {option}")
    }

    pub fn expand(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn collapse(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn set_value(&mut self, id: ElementRef, value: &str) -> Result<()> {
        todo!("{id} {value}")
    }

    pub fn value(&mut self, id: ElementRef) -> Result<Option<String>> {
        todo!("{id}")
    }

    pub fn set_focus(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn scroll_into_view(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn scroll(&mut self, id: ElementRef, direction: Direction, amount: u32) -> Result<()> {
        todo!("{id} {direction:?} {amount}")
    }

    pub fn default_action(&mut self, id: ElementRef) -> Result<()> {
        todo!("{id}")
    }

    pub fn text(&mut self, id: ElementRef, max_chars: usize) -> Result<String> {
        todo!("{id} {max_chars}")
    }
}

#[derive(Clone)]
pub struct UiaHandle {
    _private: (),
}

impl UiaHandle {
    pub fn spawn(options: UiaOptions) -> Result<Self> {
        todo!("{options:?}")
    }

    pub fn run<R, F>(&self, task: F) -> Result<R>
    where
        F: FnOnce(&mut Uia) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let _ = task;
        todo!()
    }
}
