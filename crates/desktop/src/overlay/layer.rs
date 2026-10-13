use windows::Win32::{Foundation::HINSTANCE, UI::WindowsAndMessaging::MSG};
use winpilot_core::{Point, Rect, Result};

use super::{
    canvas::Canvas,
    layer_surface::LayerSurface,
    layered_window::{is_layer_size_allowed, reposition_layer, update_layer},
    window::{Capture, OverlayWindow},
};

const ALPHA_STEP: u8 = 6;

pub(crate) struct Layer {
    window: OverlayWindow,
    surface: LayerSurface,
    placement: Option<Rect>,
    alpha: u8,
    wanted: bool,
    presented: bool,
    shown: bool,
    capture: Capture,
}

impl Layer {
    pub(crate) fn create(instance: HINSTANCE, wanted: Capture) -> Result<Self> {
        let window = OverlayWindow::create(instance)?;
        let capture = window.apply_capture(wanted);
        let surface = LayerSurface::new()?;
        Ok(Self { window, surface, placement: None, alpha: 0, wanted: false, presented: false, shown: false, capture })
    }

    pub(crate) fn is_excluded_from_capture(&self) -> bool {
        self.capture == Capture::Excluded
    }

    pub(crate) fn owns(&self, message: &MSG) -> bool {
        message.hwnd == self.window.handle()
    }

    pub(crate) fn window(&self) -> &OverlayWindow {
        &self.window
    }

    pub(crate) fn placement(&self) -> Option<Rect> {
        self.placement.filter(|_| self.wanted)
    }

    pub(crate) fn paint(&mut self, placement: Rect, alpha: u8, paint: impl FnOnce(&mut Canvas<'_>)) -> bool {
        let painted = self.present(placement, alpha, paint);
        if !painted {
            self.hide();
        }
        painted
    }

    fn present(&mut self, placement: Rect, alpha: u8, paint: impl FnOnce(&mut Canvas<'_>)) -> bool {
        if !is_layer_size_allowed(placement.width, placement.height) {
            return false;
        }
        let Ok(mut canvas) = self.surface.canvas(placement.width as usize, placement.height as usize) else {
            return false;
        };
        paint(&mut canvas);
        if update_layer(self.window.handle(), self.surface.dc(), placement, alpha).is_err() {
            return false;
        }
        self.presented = true;
        self.placement = Some(placement);
        self.alpha = alpha;
        self.wanted = true;
        true
    }

    pub(crate) fn place(&mut self, origin: Point, alpha: u8) {
        let Some(placement) = self.placement.filter(|_| self.wanted) else { return };
        let moved = (placement.x, placement.y) != (origin.x, origin.y);
        if !moved && !alpha_worth_sending(self.alpha, alpha) {
            return;
        }
        if reposition_layer(self.window.handle(), moved.then_some(origin), alpha).is_ok() {
            self.placement = Some(Rect::new(origin.x, origin.y, placement.width, placement.height));
            self.alpha = alpha;
        }
    }

    pub(crate) fn set_alpha(&mut self, alpha: u8) {
        if let Some(placement) = self.placement {
            self.place(Point::new(placement.x, placement.y), alpha);
        }
    }

    pub(crate) fn hide(&mut self) {
        self.wanted = false;
    }

    pub(crate) fn reveal(&mut self, concealed: bool) -> bool {
        let visible = should_show(self.wanted, self.presented, concealed, self.capture);
        let hiding = !visible && self.shown;
        if visible && !self.shown {
            self.window.show_on_top();
        } else if hiding {
            self.window.hide();
        }
        self.shown = visible;
        hiding
    }

    pub(crate) fn raise(&self) {
        if self.shown {
            self.window.raise();
        }
    }

    pub(crate) fn release_memory(&mut self) {
        if !self.wanted {
            self.surface.release();
            self.placement = None;
            self.presented = false;
        }
    }
}

fn should_show(wanted: bool, presented: bool, concealed: bool, capture: Capture) -> bool {
    wanted && presented && !(concealed && capture == Capture::Allowed)
}

fn alpha_worth_sending(shown: u8, wanted: u8) -> bool {
    let at_an_end = wanted == 0 || wanted == u8::MAX;
    (at_an_end && shown != wanted) || shown.abs_diff(wanted) >= ALPHA_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_only_shown_after_its_content_was_presented() {
        assert!(!should_show(true, false, false, Capture::Excluded));
        assert!(should_show(true, true, false, Capture::Excluded));
        assert!(!should_show(false, true, false, Capture::Excluded));
        assert!(should_show(true, true, true, Capture::Excluded));
        assert!(!should_show(true, true, true, Capture::Allowed));
    }

    #[test]
    fn small_alpha_changes_are_batched_but_ends_are_exact() {
        assert!(!alpha_worth_sending(140, 145));
        assert!(alpha_worth_sending(140, 146));
        assert!(alpha_worth_sending(2, 0));
        assert!(alpha_worth_sending(253, 255));
        assert!(!alpha_worth_sending(0, 0));
    }
}
