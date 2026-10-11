use std::time::Duration;

use deckhand_core::{Point, Result, glide_timing::MAX_GLIDE_MS};

use crate::{engine::Engine, overlay};

pub fn glide_duration(default_ms: u64, glide_ms: Option<u64>) -> Duration {
    Duration::from_millis(glide_ms.unwrap_or(default_ms).min(MAX_GLIDE_MS))
}

pub fn move_pointer(engine: &Engine, to: Point, glide_ms: Option<u64>) -> Result<()> {
    let duration = glide_duration(engine.config.cursor.glide_ms, glide_ms);
    let from = deckhand_input::cursor_position().unwrap_or(to);
    overlay::glide(engine, from, to, duration);
    if duration.is_zero() || from == to {
        return deckhand_input::move_to(to);
    }
    deckhand_input::glide_to(to, duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_override_the_configured_glide_and_it_is_capped() {
        assert_eq!(glide_duration(120, None), Duration::from_millis(120));
        assert_eq!(glide_duration(120, Some(0)), Duration::ZERO);
        assert_eq!(glide_duration(120, Some(400)), Duration::from_millis(400));
        assert_eq!(glide_duration(u64::MAX, None), Duration::from_millis(MAX_GLIDE_MS));
    }
}
