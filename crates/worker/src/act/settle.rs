use std::time::Duration;

use winpilot_core::WindowInfo;

use crate::{call_scope::CallScope, engine::Engine, watch::Watch};

const RESPONSIVE_WAIT: Duration = Duration::from_millis(500);

pub(crate) fn settle(engine: &Engine, window: &WindowInfo, call: &CallScope) {
    let _responsive = winpilot_desktop::wait_until_responsive(window.hwnd, call.bounded(RESPONSIVE_WAIT));
    let quiet = Duration::from_millis(engine.config.act.settle_quiet_ms);
    let max = call.bounded(Duration::from_millis(engine.config.act.settle_max_ms));
    let _quiet = Watch::window(window).wait_quiet(engine, quiet, max);
}
