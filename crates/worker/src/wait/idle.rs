use std::time::{Duration, Instant};

use winpilot_core::{Result, WindowInfo, format::window_line};

use super::{WaitReport, check::Check};
use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::{Engine, duration_ms},
    watch::Watch,
};

const SLICE: Duration = Duration::from_secs(1);

pub(crate) fn wait(engine: &Engine, quiet: Duration, timeout: Duration, call: &CallScope) -> Result<WaitReport> {
    let started = Instant::now();
    let window = default_window(engine)?;
    let watch = Watch::window(&window);
    loop {
        engine.guard.check_waiting(call)?;
        let remaining = timeout.saturating_sub(started.elapsed()).min(call.remaining());
        if watch.wait_quiet(engine, quiet, remaining.min(quiet + SLICE)) {
            return Ok(quiet_check(&window, quiet, true).report(started));
        }
        if started.elapsed() >= timeout {
            return Ok(quiet_check(&window, quiet, false).report(started));
        }
    }
}

fn quiet_check(window: &WindowInfo, quiet: Duration, calm: bool) -> Check {
    if calm {
        Check::met(format!("{} has been quiet for {} ms", window_line(window), duration_ms(quiet)))
    } else {
        Check::unmet(format!("{} is still busy", window_line(window)))
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;

    #[test]
    fn quiet_and_busy_details() {
        let window = crate::test_fixture::window(3, "code.exe", "Editor", Rect::new(0, 0, 10, 10));
        let calm = quiet_check(&window, Duration::from_millis(300), true);
        assert!(calm.met);
        assert_eq!(calm.detail, "w3 code.exe \"Editor\" has been quiet for 300 ms");
        let busy = quiet_check(&window, Duration::from_millis(300), false);
        assert!(!busy.met);
        assert_eq!(busy.detail, "w3 code.exe \"Editor\" is still busy");
    }
}
