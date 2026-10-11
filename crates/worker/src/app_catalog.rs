use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use deckhand_core::Result;
use deckhand_desktop::apps::{AppEntry, installed_apps};
use parking_lot::Mutex;

#[derive(Default)]
pub struct AppCatalog {
    cached: Mutex<Option<Listing>>,
}

struct Listing {
    loaded: Instant,
    apps: Arc<Vec<AppEntry>>,
}

impl AppCatalog {
    pub fn apps(&self, max_age: Duration) -> Result<Arc<Vec<AppEntry>>> {
        let mut cached = self.cached.lock();
        if let Some(listing) = cached.as_ref().filter(|listing| listing.loaded.elapsed() < max_age) {
            return Ok(Arc::clone(&listing.apps));
        }
        let apps = Arc::new(installed_apps()?);
        *cached = Some(Listing { loaded: Instant::now(), apps: Arc::clone(&apps) });
        Ok(apps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_a_fresh_listing_and_reloads_a_stale_one() {
        let catalog = AppCatalog::default();
        let first = catalog.apps(Duration::from_secs(300)).unwrap();
        assert!(!first.is_empty());
        let again = catalog.apps(Duration::from_secs(300)).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        let reloaded = catalog.apps(Duration::ZERO).unwrap();
        assert!(!Arc::ptr_eq(&first, &reloaded));
    }
}
