use std::time::{Duration, Instant};

use deckhand_core::{Error, ErrorCode, Point, Result, glide_timing::MAX_GLIDE_MS};

use super::{
    hit::{confirm_hit, hit_point},
    locate::Located,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const INTERRUPTED: &str =
    "the user moved the mouse or typed while Deckhand was moving the pointer, so it stopped; try again once they stop";

pub(crate) struct Motion<'a> {
    engine: &'a Engine,
    call: &'a CallScope,
    started: Instant,
}

impl<'a> Motion<'a> {
    pub(crate) fn start(engine: &'a Engine, call: &'a CallScope) -> Self {
        Self { engine, call, started: Instant::now() }
    }

    pub(crate) fn check(&self) -> Result<()> {
        self.engine.guard.check_running(self.call)?;
        if self.engine.guard.physical_input_since(self.started) {
            return Err(Error::new(ErrorCode::UserActive, INTERRUPTED));
        }
        Ok(())
    }

    pub(crate) fn glide_duration(&self, glide_ms: Option<u64>) -> Duration {
        self.call.bounded(glide_duration(self.engine.config.cursor.glide_ms, glide_ms))
    }

    pub(crate) fn glide_to(&self, to: Point, glide_ms: Option<u64>) -> Result<()> {
        self.check()?;
        let duration = self.glide_duration(glide_ms);
        let from = deckhand_input::cursor_position().unwrap_or(to);
        overlay::glide(self.engine, from, to, duration);
        if duration.is_zero() || from == to {
            return deckhand_input::move_to(to);
        }
        deckhand_input::glide_to_guarded(to, duration, || self.check())
    }

    pub(crate) fn approach(&self, prepared: &Prepared, located: &Located, glide_ms: Option<u64>) -> Result<Point> {
        let point = hit_point(self.engine, prepared, located)?;
        overlay::begin(self.engine, &located.window, self.call);
        self.glide_to(point, glide_ms)?;
        confirm_hit(self.engine, prepared, located, point)?;
        self.check()?;
        Ok(point)
    }
}

fn glide_duration(default_ms: u64, glide_ms: Option<u64>) -> Duration {
    Duration::from_millis(glide_ms.unwrap_or(default_ms).min(MAX_GLIDE_MS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cancel::Cancel, test_fixture::engine};

    #[test]
    fn steps_override_the_configured_glide_and_it_is_capped() {
        assert_eq!(glide_duration(120, None), Duration::from_millis(120));
        assert_eq!(glide_duration(120, Some(0)), Duration::ZERO);
        assert_eq!(glide_duration(120, Some(400)), Duration::from_millis(400));
        assert_eq!(glide_duration(u64::MAX, None), Duration::from_millis(MAX_GLIDE_MS));
    }

    #[test]
    fn a_glide_never_outlasts_the_call() {
        let call = CallScope::new(Cancel::default(), Instant::now() + Duration::from_millis(50));
        let motion = Motion::start(engine(), &call);
        assert!(motion.glide_duration(Some(3000)) <= Duration::from_millis(50));
        assert!(motion.glide_duration(None) <= Duration::from_millis(50));
    }
}
