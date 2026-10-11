use super::client::Overlay;

#[must_use = "the overlay comes back as soon as the guard is dropped"]
pub struct CaptureHideGuard<'a> {
    overlay: Option<&'a Overlay>,
    confirmed: bool,
}

impl<'a> CaptureHideGuard<'a> {
    pub(crate) const fn not_needed() -> Self {
        Self { overlay: None, confirmed: true }
    }

    pub(crate) const fn hiding(overlay: &'a Overlay, confirmed: bool) -> Self {
        Self { overlay: Some(overlay), confirmed }
    }

    pub fn is_hiding(&self) -> bool {
        self.overlay.is_some()
    }

    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }
}

impl Drop for CaptureHideGuard<'_> {
    fn drop(&mut self) {
        if let Some(overlay) = self.overlay {
            overlay.unconceal();
        }
    }
}
