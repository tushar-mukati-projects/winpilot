use std::time::{Duration, Instant};

use deckhand_core::{Point, Rect};

use super::{
    chip_text::ChipText,
    easing::{breath, lap_fraction, progress},
    fade::Fade,
    focus_outline::{FOCUS_FLOW, FOCUS_SHIMMER, FocusLook, STILL_ALPHA, focus_alpha},
    presence::{BREATH, FRAME},
    ripple::{STILL_LIFE, ripple_life},
    scan::{SCAN_SWEEP, STILL_SCAN},
    scene_frame::{ChipsFrame, FocusFrame, RecordingFrame, RippleFrame, ScanFrame, TrailFrame, ViewfinderFrame},
    trail::{TrailDot, trail_ends},
    viewfinder::{STILL_VIEWFINDER_LIFE, VIEWFINDER_LIFE},
};

pub(crate) const CHIPS_SHOWN: Duration = Duration::from_millis(900);
const CHIPS_FADE: Duration = Duration::from_millis(200);

pub(crate) const RECORDING_FRAME: Duration = Duration::from_millis(33);
const SECOND: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    pub(crate) scale: f32,
    pub(crate) animated: bool,
}

fn age(started: Instant, now: Instant) -> Duration {
    now.saturating_duration_since(started)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ripple {
    pub(crate) center: Point,
    pub(crate) rings: u8,
    pub(crate) look: Look,
    pub(crate) started: Instant,
    pub(crate) serial: u64,
}

impl Ripple {
    fn life(&self) -> Duration {
        if self.look.animated { ripple_life(self.rings) } else { STILL_LIFE }
    }

    pub(crate) fn is_over(&self, now: Instant) -> bool {
        age(self.started, now) >= self.life()
    }

    pub(crate) fn frame(&self, now: Instant) -> RippleFrame {
        RippleFrame {
            center: self.center,
            elapsed: age(self.started, now),
            rings: self.rings,
            scale: self.look.scale,
            animated: self.look.animated,
            serial: self.serial,
        }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        if self.look.animated { FRAME } else { self.life().saturating_sub(age(self.started, now)) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Trail {
    pub(crate) dots: Vec<TrailDot>,
    pub(crate) started: Instant,
    pub(crate) serial: u64,
}

impl Trail {
    pub(crate) fn is_over(&self, now: Instant) -> bool {
        age(self.started, now) >= trail_ends(&self.dots)
    }

    pub(crate) fn frame(&self, now: Instant) -> TrailFrame<'_> {
        TrailFrame { dots: &self.dots, elapsed: age(self.started, now), serial: self.serial }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Focus {
    pub(crate) element: Rect,
    pub(crate) look: Look,
    pub(crate) started: Instant,
    pub(crate) fade: Fade,
}

impl Focus {
    pub(crate) fn frame(&self, now: Instant) -> FocusFrame {
        let elapsed = age(self.started, now);
        let breathing = if self.look.animated { focus_alpha(elapsed) } else { STILL_ALPHA };
        let look = if self.look.animated {
            FocusLook { flow: lap_fraction(elapsed, FOCUS_FLOW), shimmer: lap_fraction(elapsed, FOCUS_SHIMMER) }
        } else {
            FocusLook { flow: 0.0, shimmer: 0.125 }
        };
        FocusFrame {
            element: self.element,
            scale: self.look.scale,
            alpha: breathing * self.fade.level(now),
            look,
            steady: self.fade.is_settled(now),
        }
    }

    pub(crate) fn wake(&self, now: Instant) -> Option<Duration> {
        (self.look.animated || !self.fade.is_settled(now)).then_some(FRAME)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Recording {
    pub(crate) rect: Rect,
    pub(crate) look: Look,
    pub(crate) started: Instant,
}

impl Recording {
    pub(crate) fn frame(&self, now: Instant) -> RecordingFrame {
        let elapsed = age(self.started, now);
        let pulse = if self.look.animated { breath(elapsed, BREATH) } else { 1.0 };
        RecordingFrame { rect: self.rect, scale: self.look.scale, seconds: elapsed.as_secs(), pulse }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        if self.look.animated {
            return RECORDING_FRAME;
        }
        let into_second = Duration::from_nanos(u64::from(age(self.started, now).subsec_nanos()));
        SECOND.saturating_sub(into_second)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Viewfinder {
    pub(crate) rect: Rect,
    pub(crate) look: Look,
    pub(crate) started: Instant,
}

impl Viewfinder {
    fn life(&self) -> Duration {
        if self.look.animated { VIEWFINDER_LIFE } else { STILL_VIEWFINDER_LIFE }
    }

    pub(crate) fn is_over(&self, now: Instant) -> bool {
        age(self.started, now) >= self.life()
    }

    pub(crate) fn frame(&self, now: Instant) -> ViewfinderFrame {
        ViewfinderFrame {
            rect: self.rect,
            scale: self.look.scale,
            elapsed: age(self.started, now),
            animated: self.look.animated,
        }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        if self.look.animated { FRAME } else { self.life().saturating_sub(age(self.started, now)) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chips {
    pub(crate) rect: Rect,
    pub(crate) text: ChipText,
    pub(crate) look: Look,
    pub(crate) started: Instant,
    pub(crate) serial: u64,
}

impl Chips {
    fn life(&self) -> Duration {
        if self.look.animated { CHIPS_SHOWN + CHIPS_FADE } else { CHIPS_SHOWN }
    }

    pub(crate) fn is_over(&self, now: Instant) -> bool {
        age(self.started, now) >= self.life()
    }

    pub(crate) fn frame(&self, now: Instant) -> ChipsFrame<'_> {
        let fading = age(self.started, now).saturating_sub(CHIPS_SHOWN);
        let alpha = 1.0 - progress(fading, CHIPS_FADE);
        ChipsFrame { rect: self.rect, text: &self.text, alpha, scale: self.look.scale, serial: self.serial }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        let elapsed = age(self.started, now);
        if elapsed >= CHIPS_SHOWN && self.look.animated {
            return FRAME;
        }
        self.life().min(CHIPS_SHOWN).saturating_sub(elapsed).max(FRAME)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Scan {
    pub(crate) rect: Rect,
    pub(crate) look: Look,
    pub(crate) started: Instant,
}

impl Scan {
    fn life(&self) -> Duration {
        if self.look.animated { SCAN_SWEEP } else { STILL_SCAN }
    }

    pub(crate) fn is_over(&self, now: Instant) -> bool {
        age(self.started, now) >= self.life()
    }

    pub(crate) fn frame(&self, now: Instant) -> ScanFrame {
        ScanFrame {
            rect: self.rect,
            scale: self.look.scale,
            elapsed: age(self.started, now),
            animated: self.look.animated,
        }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        if self.look.animated { FRAME } else { self.life().saturating_sub(age(self.started, now)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const ANIMATED: Look = Look { scale: 1.0, animated: true };
    const STILL: Look = Look { scale: 1.0, animated: false };

    #[test]
    fn recording_counts_whole_seconds_and_pulses() {
        let start = Instant::now();
        let recording = Recording { rect: Rect::new(0, 0, 800, 600), look: ANIMATED, started: start };
        assert_eq!(recording.frame(start + MS(12_400)).seconds, 12);
        assert_eq!(recording.wake(start), RECORDING_FRAME);
        let pulses: Vec<f32> = (0..16).map(|step| recording.frame(start + MS(step * 100)).pulse).collect();
        assert!(pulses.iter().any(|pulse| *pulse > 0.9) && pulses.iter().any(|pulse| *pulse < 0.1));
    }

    #[test]
    fn a_still_recording_wakes_once_a_second() {
        let start = Instant::now();
        let recording = Recording { rect: Rect::new(0, 0, 800, 600), look: STILL, started: start };
        assert_eq!(recording.wake(start + MS(2_300)), MS(700));
        assert!(crate::overlay::easing::close(recording.frame(start + MS(2_300)).pulse, 1.0));
    }

    #[test]
    fn keystroke_chips_show_for_900_ms_then_fade() {
        let start = Instant::now();
        let chips = Chips {
            rect: Rect::new(0, 0, 500, 400),
            text: ChipText::typing(),
            look: ANIMATED,
            started: start,
            serial: 1,
        };
        assert!(crate::overlay::easing::close(chips.frame(start + MS(800)).alpha, 1.0));
        assert!(chips.frame(start + MS(1000)).alpha < 0.6);
        assert_eq!(chips.wake(start + MS(100)), MS(800));
        assert!(!chips.is_over(start + MS(1050)) && chips.is_over(start + MS(1100)));
        assert!(Chips { look: STILL, ..chips.clone() }.is_over(start + MS(900)));
    }

    #[test]
    fn a_scan_sweeps_once() {
        let start = Instant::now();
        let scan = Scan { rect: Rect::new(0, 0, 500, 400), look: ANIMATED, started: start };
        assert!(!scan.is_over(start + MS(400)) && scan.is_over(start + MS(420)));
        assert_eq!(scan.wake(start), FRAME);
        assert_eq!(Scan { look: STILL, ..scan }.wake(start + MS(100)), MS(200));
    }

    #[test]
    fn the_viewfinder_lasts_420_ms_or_200_when_still() {
        let start = Instant::now();
        let moving = Viewfinder { rect: Rect::new(0, 0, 10, 10), look: ANIMATED, started: start };
        assert!(!moving.is_over(start + MS(419)) && moving.is_over(start + MS(420)));
        let still = Viewfinder { look: STILL, ..moving };
        assert!(still.is_over(start + MS(200)));
        assert_eq!(still.wake(start + MS(50)), MS(150));
    }
}
