use std::time::{Duration, Instant};

use winpilot_core::{Result, WindowInfo, ocr::find_matches};

use super::check::{Presence, Shown, unless_gone};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    ocr_read::{Freshness, window_lines, window_matches},
};

const OCR_EVERY: Duration = Duration::from_millis(400);

struct Paced<T> {
    every: Duration,
    last: Option<(Instant, T)>,
}

impl<T: Clone> Paced<T> {
    const fn new(every: Duration) -> Self {
        Self { every, last: None }
    }

    fn get(&mut self, now: Instant, compute: impl FnOnce() -> Result<T>) -> Result<T> {
        if let Some((at, value)) = &self.last
            && now.saturating_duration_since(*at) < self.every
        {
            return Ok(value.clone());
        }
        let value = compute()?;
        self.last = Some((now, value.clone()));
        Ok(value)
    }
}

pub(crate) struct OcrLook<'a> {
    call: &'a CallScope,
    paced: Paced<Presence>,
}

impl<'a> OcrLook<'a> {
    pub(crate) const fn new(call: &'a CallScope) -> Self {
        Self { call, paced: Paced::new(OCR_EVERY) }
    }

    pub(crate) fn presence(
        &mut self,
        engine: &Engine,
        windows: &[WindowInfo],
        text: &str,
        nth: Option<u32>,
    ) -> Result<Presence> {
        let call = self.call;
        let confirm_miss = confirms_a_miss(self.paced.last.as_ref().map(|(_, last)| last));
        self.paced.get(Instant::now(), || read_presence(engine, windows, text, nth, call, confirm_miss))
    }
}

fn confirms_a_miss(last: Option<&Presence>) -> bool {
    last.is_none_or(|last| last.shown.is_some())
}

fn read_presence(
    engine: &Engine,
    windows: &[WindowInfo],
    text: &str,
    nth: Option<u32>,
    call: &CallScope,
    confirm_miss: bool,
) -> Result<Presence> {
    let wanted = position(nth);
    for window in windows {
        let fresh = Freshness::TakenAfter(Instant::now());
        let read = if confirm_miss {
            window_matches(engine, window, text, Some(call), fresh)
        } else {
            window_lines(engine, window, Some(call), fresh).map(|lines| find_matches(&lines, text))
        };
        let Some(matches) = unless_gone(read)? else { continue };
        if let Some(found) = matches.into_iter().nth(wanted) {
            return Ok(Presence::certain(Some(Shown::Text { text: found.text, rect: found.rect })));
        }
    }
    Ok(Presence::certain(None))
}

fn position(nth: Option<u32>) -> usize {
    nth.and_then(|nth| usize::try_from(nth).ok()).unwrap_or(1).saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use winpilot_core::Error;

    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn a_paced_value_is_reused_until_its_interval_passes() {
        let computed = Cell::new(0);
        let mut paced = Paced::new(MS(400));
        let start = Instant::now();
        let mut read = |at: Instant| {
            paced
                .get(at, || {
                    computed.set(computed.get() + 1);
                    Ok(computed.get())
                })
                .unwrap()
        };
        assert_eq!(read(start), 1);
        assert_eq!(read(start + MS(100)), 1);
        assert_eq!(read(start + MS(399)), 1);
        assert_eq!(read(start + MS(400)), 2);
        assert_eq!(read(start + MS(500)), 2);
        assert_eq!(computed.get(), 2);
    }

    #[test]
    fn a_failed_reading_is_not_remembered() {
        let mut paced: Paced<u8> = Paced::new(MS(400));
        let start = Instant::now();
        assert!(paced.get(start, || Err(Error::unsupported("no OCR language installed"))).is_err());
        assert_eq!(paced.get(start + MS(10), || Ok(7)).unwrap(), 7);
    }

    #[test]
    fn a_miss_is_confirmed_by_every_pass_unless_the_text_was_already_missing() {
        let shown = Shown::Text { text: "Export".to_string(), rect: winpilot_core::Rect::new(1, 2, 3, 4) };
        assert!(confirms_a_miss(None));
        assert!(confirms_a_miss(Some(&Presence::certain(Some(shown)))));
        assert!(!confirms_a_miss(Some(&Presence::certain(None))));
    }

    #[test]
    fn nth_counts_from_one() {
        assert_eq!(position(None), 0);
        assert_eq!(position(Some(1)), 0);
        assert_eq!(position(Some(3)), 2);
    }
}
