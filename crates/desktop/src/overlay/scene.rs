#[cfg(test)]
mod scene_tests;
mod timeline;

use std::time::{Duration, Instant};

use winpilot_core::{Point, Rect};

use super::{
    activity_pill::ActivityPill,
    chip_text::ChipText,
    fade::Fade,
    pointer_badge::PointerBadge,
    presence::{Driving, Presence, timed},
    scan::SCAN_COOLDOWN,
    scene_frame::MAX_RIPPLES,
    scene_items::{Chips, Focus, Look, Recording, Ripple, Scan, Trail, Viewfinder},
    trail::{TrailDot, glide_dots, trail_dots},
};

const FOCUS_FADE: Duration = Duration::from_millis(150);
const REMEMBERED_SCANS: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct Scene {
    presence: Option<Presence>,
    activity: Option<ActivityPill>,
    ripples: [Option<Ripple>; MAX_RIPPLES],
    trail: Option<Trail>,
    focus: Option<Focus>,
    recording: Option<Recording>,
    viewfinder: Option<Viewfinder>,
    pointer: Option<PointerBadge>,
    chips: Option<Chips>,
    scan: Option<Scan>,
    recent_scans: Vec<(Rect, Instant)>,
    serial: u64,
    halted: bool,
}

impl Scene {
    pub(crate) fn begin(&mut self, rect: Rect, look: Look, driving: Driving, now: Instant) {
        match (&mut self.presence, self.halted) {
            (Some(presence), true) => presence.retarget_stopped(rect, now),
            (Some(presence), false) => presence.retarget(rect, look.scale, look.animated, now),
            (None, true) => self.presence = Some(Presence::stopped(rect, look.scale, look.animated, now)),
            (None, false) => self.presence = Some(Presence::working(rect, look.scale, look.animated, now)),
        }
        if let Some(presence) = &mut self.presence {
            presence.drive(driving);
        }
    }

    pub(crate) fn activity(&mut self, area: Rect, hint: String, look: Look, now: Instant) {
        if self.halted {
            return;
        }
        self.end(now);
        match &mut self.activity {
            Some(activity) => activity.update(area, hint, look, now),
            None => self.activity = Some(ActivityPill::new(area, hint, look, now)),
        }
    }

    pub(crate) fn end(&mut self, now: Instant) {
        if let Some(presence) = &mut self.presence {
            presence.leave(now);
        }
        if let Some(pointer) = &mut self.pointer {
            pointer.leave(now);
        }
        self.clear_focus(now);
    }

    pub(crate) fn stopped(&mut self, banner_area: Rect, look: Look, now: Instant) {
        self.halted = true;
        match &mut self.presence {
            Some(presence) => presence.stop(now),
            None => self.presence = Some(Presence::stopped_banner(banner_area, look.scale, look.animated, now)),
        }
        self.focus = None;
        self.trail = None;
        self.chips = None;
        if let Some(pointer) = &mut self.pointer {
            pointer.leave(now);
        }
        if let Some(activity) = &mut self.activity {
            activity.dismiss(now);
        }
    }

    pub(crate) fn resumed(&mut self, now: Instant) {
        self.halted = false;
        if let Some(presence) = &mut self.presence {
            presence.resume(now);
        }
    }

    pub(crate) fn click(&mut self, center: Point, count: u8, look: Look, now: Instant) {
        self.point_at(center, center, Duration::ZERO, look, now);
        self.ripple(center, count, look, now);
    }

    pub(crate) fn ripple(&mut self, center: Point, count: u8, look: Look, now: Instant) {
        self.clear_focus(now);
        let serial = self.next_serial();
        let ripple = Ripple { center, rings: count.clamp(1, 2), look, started: now, serial };
        let slot = self.free_ripple_slot();
        if let Some(entry) = self.ripples.get_mut(slot) {
            *entry = Some(ripple);
        }
    }

    fn trail(&mut self, from: Point, to: Point, look: Look, now: Instant) {
        self.point_at(to, to, Duration::ZERO, look, now);
        let dots = if look.animated { trail_dots(from, to, look.scale) } else { Vec::new() };
        self.show_trail(dots, now);
    }

    pub(crate) fn glide(&mut self, from: Point, to: Point, duration: Duration, look: Look, now: Instant) {
        if duration.is_zero() {
            return self.trail(from, to, look, now);
        }
        self.point_at(from, to, duration, look, now);
        let dots = if look.animated { glide_dots(from, to, duration, look.scale) } else { Vec::new() };
        self.show_trail(dots, now);
    }

    pub(crate) fn focus_field(&mut self, element: Rect, look: Look, now: Instant) {
        let same = self.focus.is_some_and(|focus| focus.element == element && !focus.fade.is_leaving());
        if !same {
            let fade = Fade::appear(timed(FOCUS_FADE, look.animated), now);
            self.focus = Some(Focus { element, look, started: now, fade });
        }
    }

    pub(crate) fn recording(&mut self, rect: Rect, look: Look, now: Instant) {
        let started = self.recording.map_or(now, |recording| recording.started);
        self.recording = Some(Recording { rect, look, started });
    }

    pub(crate) fn keystrokes(&mut self, rect: Rect, text: ChipText, look: Look, now: Instant) {
        let serial = self.next_serial();
        self.chips = Some(Chips { rect, text, look, started: now, serial });
    }

    pub(crate) fn scan(&mut self, rect: Rect, look: Look, now: Instant) -> bool {
        let acting = self.presence.is_some_and(|presence| presence.is_acting());
        let running = self.scan.is_some_and(|scan| scan.rect == rect && !scan.is_over(now));
        self.recent_scans.retain(|(_, when)| now.saturating_duration_since(*when) < SCAN_COOLDOWN);
        let recent = self.recent_scans.iter().any(|(seen, _)| *seen == rect);
        if acting || running || recent || rect.is_empty() {
            return false;
        }
        if self.recent_scans.len() >= REMEMBERED_SCANS {
            self.recent_scans.remove(0);
        }
        self.recent_scans.push((rect, now));
        self.scan = Some(Scan { rect, look, started: now });
        true
    }

    pub(crate) fn recording_stopped(&mut self) {
        self.recording = None;
    }

    pub(crate) fn screenshot(&mut self, rect: Rect, look: Look, now: Instant) {
        self.viewfinder = Some(Viewfinder { rect, look, started: now });
    }

    fn point_at(&mut self, from: Point, to: Point, duration: Duration, look: Look, now: Instant) {
        if !look.animated {
            return;
        }
        match &mut self.pointer {
            Some(pointer) => pointer.go(from, to, duration, look, now),
            None => self.pointer = Some(PointerBadge::new(from, to, duration, look, now)),
        }
    }

    fn show_trail(&mut self, dots: Vec<TrailDot>, now: Instant) {
        if dots.is_empty() {
            return;
        }
        let serial = self.next_serial();
        self.trail = Some(Trail { dots, started: now, serial });
    }

    fn next_serial(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }

    fn clear_focus(&mut self, now: Instant) {
        let Some(focus) = &mut self.focus else { return };
        if !focus.fade.is_leaving() {
            focus.fade = focus.fade.toward(0.0, timed(FOCUS_FADE, focus.look.animated), now);
        }
    }

    fn free_ripple_slot(&self) -> usize {
        if let Some(free) = self.ripples.iter().position(Option::is_none) {
            return free;
        }
        let oldest =
            self.ripples.iter().enumerate().filter_map(|(index, slot)| slot.map(|ripple| (ripple.serial, index)));
        oldest.min().map_or(0, |(_, index)| index)
    }
}
