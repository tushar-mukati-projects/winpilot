mod check;
mod idle;
mod pause;
mod presence;
mod text;
mod windows;

#[cfg(test)]
mod fixture_tests;

use std::time::{Duration, Instant};

use deckhand_core::{Element, Result, protocol::WaitCondition};

use self::check::{Check, appears, disappears, target_label};
pub use self::{
    check::{is_gone, unless_gone},
    windows::WindowsBefore,
};
use crate::{call_scope::CallScope, engine::Engine, watch::Watch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitReport {
    pub satisfied: bool,
    pub elapsed_ms: u64,
    pub detail: String,
    pub element: Option<Element>,
}

pub fn until(
    engine: &Engine,
    condition: &WaitCondition,
    timeout: Duration,
    before: &WindowsBefore,
    call: &CallScope,
) -> Result<WaitReport> {
    match condition {
        WaitCondition::Time { wanted } => pause::pause(engine, *wanted, call),
        WaitCondition::Appears { target, window } => {
            let (label, scope) = (target_label(target), window.as_deref());
            poll(engine, timeout, &presence::watch(engine, target, scope), call, || {
                presence::locate(engine, target, scope).map(|found| appears(&label, found))
            })
        }
        WaitCondition::Disappears { target, window } => {
            let (label, scope) = (target_label(target), window.as_deref());
            poll(engine, timeout, &presence::watch(engine, target, scope), call, || {
                presence::locate(engine, target, scope).map(|found| disappears(&label, found))
            })
        }
        WaitCondition::WindowOpens { title } => {
            poll(engine, timeout, &Watch::any(), call, || windows::opens(engine, title, before))
        }
        WaitCondition::WindowCloses { title } => {
            poll(engine, timeout, &Watch::any(), call, || windows::closes(engine, title))
        }
        WaitCondition::Text { text, window } => {
            let scope = window.as_deref();
            poll(engine, timeout, &text::watch(engine, scope), call, || text::check(engine, text, scope))
        }
        WaitCondition::Idle { quiet } => idle::wait(engine, *quiet, timeout, call),
    }
}

fn poll(
    engine: &Engine,
    timeout: Duration,
    watch: &Watch,
    call: &CallScope,
    mut evaluate: impl FnMut() -> Result<Check>,
) -> Result<WaitReport> {
    let started = Instant::now();
    let mut latest = None;
    let met = crate::until::until(engine, call, timeout, watch, || {
        let check = evaluate()?;
        if check.met {
            return Ok(Some(check));
        }
        latest = Some(check);
        Ok(None)
    })?;
    let check = met.or(latest).unwrap_or_else(|| Check::unmet("the condition was never checked"));
    Ok(check.report(started))
}
