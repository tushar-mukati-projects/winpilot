use std::time::{Duration, Instant};

use winpilot_core::{Point, Rect};

use super::{
    activity_pill::ActivityPill,
    chip_text::ChipText,
    driving::Driving,
    fade::Fade,
    pointer_badge::PointerBadge,
    presence::{FRAME, Presence, timed},
    scan::SCAN_COOLDOWN,
    scene_frame::{Frame, MAX_RIPPLES},
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

    pub(crate) fn advance(&mut self, now: Instant) {
        if let Some(presence) = &mut self.presence {
            presence.advance(now);
        }
        if self.presence.is_some_and(|presence| presence.is_gone(now)) {
            self.presence = None;
        }
        if let Some(activity) = &mut self.activity {
            activity.advance(now);
        }
        if self.activity.as_ref().is_some_and(|activity| activity.is_gone(now)) {
            self.activity = None;
        }
        for slot in &mut self.ripples {
            if slot.is_some_and(|ripple| ripple.is_over(now)) {
                *slot = None;
            }
        }
        if self.trail.as_ref().is_some_and(|trail| trail.is_over(now)) {
            self.trail = None;
        }
        if self.focus.is_some_and(|focus| focus.fade.has_vanished(now)) {
            self.focus = None;
        }
        if self.viewfinder.is_some_and(|viewfinder| viewfinder.is_over(now)) {
            self.viewfinder = None;
        }
        if let Some(pointer) = &mut self.pointer {
            pointer.advance(now);
        }
        if self.pointer.is_some_and(|pointer| pointer.is_gone(now)) {
            self.pointer = None;
        }
        if self.chips.as_ref().is_some_and(|chips| chips.is_over(now)) {
            self.chips = None;
        }
        if self.scan.is_some_and(|scan| scan.is_over(now)) {
            self.scan = None;
        }
    }

    pub(crate) fn frame(&self, now: Instant) -> Frame<'_> {
        Frame {
            border: self.presence.and_then(|presence| presence.border(now)),
            pill: self.presence.map(|presence| presence.pill(now)),
            activity: self.activity.as_ref().map(|activity| activity.frame(now)),
            ripples: self.ripples.map(|slot| slot.map(|ripple| ripple.frame(now))),
            trail: self.trail.as_ref().map(|trail| trail.frame(now)),
            focus: self.focus.map(|focus| focus.frame(now)),
            recording: self.recording.map(|recording| recording.frame(now)),
            viewfinder: self.viewfinder.map(|viewfinder| viewfinder.frame(now)),
            badge: self.pointer.map(|pointer| pointer.frame(now)),
            chips: self.chips.as_ref().map(|chips| chips.frame(now)),
            scan: self.scan.map(|scan| scan.frame(now)),
        }
    }

    pub(crate) fn next_wake(&self, now: Instant) -> Option<Duration> {
        let presence = self.presence.and_then(|presence| presence.next_wake(now));
        let activity = self.activity.as_ref().map(|activity| activity.wake(now));
        let ripples = self.ripples.iter().flatten().map(|ripple| ripple.wake(now));
        let trail = self.trail.as_ref().map(|_| FRAME);
        let focus = self.focus.and_then(|focus| focus.wake(now));
        let recording = self.recording.map(|recording| recording.wake(now));
        let viewfinder = self.viewfinder.map(|viewfinder| viewfinder.wake(now));
        let pointer = self.pointer.map(|pointer| pointer.wake(now));
        let chips = self.chips.as_ref().map(|chips| chips.wake(now));
        let scan = self.scan.map(|scan| scan.wake(now));
        let effects = [activity, trail, recording, viewfinder, pointer, chips, scan];
        presence.into_iter().chain(ripples).chain(focus).chain(effects.into_iter().flatten()).min()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.presence.is_none()
            && self.activity.is_none()
            && self.ripples.iter().all(Option::is_none)
            && self.trail.is_none()
            && self.focus.is_none()
            && self.recording.is_none()
            && self.viewfinder.is_none()
            && self.pointer.is_none()
            && self.chips.is_none()
            && self.scan.is_none()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{presence::Tone, ripple::STILL_LIFE, scene_items::RECORDING_FRAME};

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const ANIMATED: Look = Look { scale: 1.0, animated: true };
    const STILL: Look = Look { scale: 1.0, animated: false };
    const RECT: Rect = Rect::new(100, 100, 800, 600);

    #[test]
    fn an_empty_scene_never_wakes() {
        let scene = Scene::default();
        assert!(scene.is_empty());
        assert_eq!(scene.next_wake(Instant::now()), None);
        assert_eq!(scene.frame(Instant::now()), Frame::default());
    }

    #[test]
    fn begin_animates_until_end_then_goes_idle() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.begin(RECT, ANIMATED, Driving::Foreground, start);
        assert_eq!(scene.next_wake(start), Some(FRAME));
        scene.end(start + MS(1000));
        scene.advance(start + MS(1300));
        assert!(scene.is_empty());
        assert_eq!(scene.next_wake(start + MS(1300)), None);
    }

    #[test]
    fn background_ripples_carry_no_pointer_badge() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.begin(RECT, ANIMATED, Driving::Background, start);
        scene.ripple(Point::new(300, 300), 1, ANIMATED, start);
        let frame = scene.frame(start + MS(50));
        assert!(frame.badge.is_none() && frame.ripples[0].is_some());
        assert_eq!(frame.pill.map(|pill| pill.driving), Some(Driving::Background));
        scene.click(Point::new(300, 300), 1, ANIMATED, start + MS(60));
        assert!(scene.frame(start + MS(70)).badge.is_some());
    }

    #[test]
    fn background_activity_replaces_the_window_frame_and_fades_after_two_seconds() {
        let start = Instant::now();
        let mut scene = Scene::default();
        let area = Rect::new(0, 0, 1920, 1032);
        scene.begin(RECT, ANIMATED, Driving::Background, start);
        scene.activity(area, " \u{00B7} working in CapCut".to_string(), ANIMATED, start + MS(100));
        scene.advance(start + MS(500));
        let frame = scene.frame(start + MS(500));
        assert!(frame.pill.is_none() && frame.border.is_none());
        let shown = frame.activity.map(|activity| (activity.anchor, activity.hint));
        assert_eq!(shown, Some((area, " \u{00B7} working in CapCut")));
        scene.advance(start + MS(2100));
        assert!(scene.frame(start + MS(2100)).activity.is_some());
        scene.advance(start + MS(2450));
        assert!(scene.is_empty());
    }

    #[test]
    fn the_kill_switch_dismisses_activity_and_ignores_more() {
        let start = Instant::now();
        let mut scene = Scene::default();
        let area = Rect::new(0, 0, 1920, 1032);
        scene.activity(area, " \u{00B7} working in CapCut".to_string(), ANIMATED, start);
        scene.stopped(area, ANIMATED, start + MS(100));
        scene.activity(area, " \u{00B7} click".to_string(), ANIMATED, start + MS(200));
        scene.advance(start + MS(450));
        let frame = scene.frame(start + MS(450));
        assert!(frame.activity.is_none());
        assert_eq!(frame.pill.map(|pill| pill.tone), Some(Tone::Stopped));
    }

    #[test]
    fn ripples_are_pooled_and_the_oldest_is_recycled() {
        let start = Instant::now();
        let mut scene = Scene::default();
        for index in 0..5 {
            scene.click(Point::new(index * 100, 0), 1, ANIMATED, start + MS(index as u64));
        }
        let frame = scene.frame(start + MS(10));
        let centers: Vec<i32> = frame.ripples.iter().flatten().map(|ripple| ripple.center.x).collect();
        assert_eq!(centers.len(), MAX_RIPPLES);
        assert!(!centers.contains(&0) && centers.contains(&400));
        scene.advance(start + MS(430));
        assert!(scene.frame(start + MS(430)).ripples.iter().all(Option::is_none));
    }

    #[test]
    fn double_clicks_live_longer() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.click(Point::new(5, 5), 2, ANIMATED, start);
        scene.advance(start + MS(400));
        assert_eq!(scene.frame(start + MS(480)).ripples[0].map(|ripple| ripple.rings), Some(2));
        scene.advance(start + MS(510));
        assert!(scene.frame(start + MS(510)).ripples.iter().all(Option::is_none));
    }

    #[test]
    fn a_click_clears_the_focus_field() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.focus_field(Rect::new(10, 10, 100, 20), ANIMATED, start);
        assert!(scene.frame(start + MS(200)).focus.is_some_and(|focus| focus.alpha > 0.3));
        scene.click(Point::new(50, 20), 1, ANIMATED, start + MS(300));
        scene.advance(start + MS(460));
        assert!(scene.frame(start + MS(460)).focus.is_none());
    }

    #[test]
    fn short_or_still_trails_are_skipped() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.trail(Point::new(0, 0), Point::new(10, 0), ANIMATED, start);
        scene.trail(Point::new(0, 0), Point::new(500, 0), STILL, start);
        scene.glide(Point::new(0, 0), Point::new(500, 0), MS(120), STILL, start);
        assert!(scene.frame(start).trail.is_none());
        scene.trail(Point::new(0, 0), Point::new(500, 0), ANIMATED, start);
        assert!(scene.frame(start).trail.is_some_and(|trail| trail.dots.len() >= 6));
        scene.advance(start + MS(260));
        assert!(scene.frame(start + MS(260)).trail.is_none());
    }

    #[test]
    fn a_glide_trail_lives_until_its_last_dot_fades() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.glide(Point::new(0, 0), Point::new(500, 0), MS(300), ANIMATED, start);
        let trail = scene.frame(start + MS(10)).trail.unwrap();
        assert!(trail.dots.iter().any(|dot| dot.alpha(trail.elapsed) == 0.0));
        scene.advance(start + MS(400));
        assert!(scene.frame(start + MS(400)).trail.is_some());
        scene.advance(start + MS(560));
        assert!(scene.frame(start + MS(560)).trail.is_none());
    }

    #[test]
    fn stopping_with_nothing_showing_shows_only_the_banner() {
        let start = Instant::now();
        let mut scene = Scene::default();
        let area = Rect::new(0, 0, 1920, 1032);
        scene.stopped(area, ANIMATED, start);
        let frame = scene.frame(start + MS(200));
        assert!(frame.border.is_none());
        assert_eq!(frame.pill.map(|pill| (pill.tone, pill.anchor)), Some((Tone::Stopped, area)));
        scene.end(start + MS(300));
        scene.advance(start + MS(1000));
        assert!(!scene.is_empty());
        scene.advance(start + MS(1600));
        scene.advance(start + MS(1850));
        assert!(scene.is_empty());
    }

    #[test]
    fn a_later_begin_keeps_showing_stopped_until_resumed() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.begin(RECT, ANIMATED, Driving::Foreground, start);
        scene.stopped(Rect::new(0, 0, 1920, 1032), ANIMATED, start + MS(100));
        scene.begin(Rect::new(50, 50, 600, 400), ANIMATED, Driving::Foreground, start + MS(300));
        let tone = |scene: &Scene, at: Instant| scene.frame(at).pill.map(|pill| pill.tone);
        assert_eq!(tone(&scene, start + MS(400)), Some(Tone::Stopped));
        scene.advance(start + MS(1800));
        scene.advance(start + MS(2100));
        assert!(scene.is_empty());
        scene.begin(RECT, ANIMATED, Driving::Foreground, start + MS(2200));
        assert_eq!(tone(&scene, start + MS(2300)), Some(Tone::Stopped));
        assert!(scene.frame(start + MS(2300)).border.is_some_and(|border| border.tone == Tone::Stopped));
        scene.resumed(start + MS(2400));
        scene.begin(RECT, ANIMATED, Driving::Foreground, start + MS(2500));
        assert_eq!(tone(&scene, start + MS(2600)), Some(Tone::Working));
    }

    #[test]
    fn resuming_before_the_banner_fades_lets_the_next_begin_work() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.begin(RECT, ANIMATED, Driving::Foreground, start);
        scene.stopped(Rect::new(0, 0, 1920, 1032), ANIMATED, start + MS(100));
        scene.resumed(start + MS(500));
        scene.begin(RECT, ANIMATED, Driving::Foreground, start + MS(550));
        assert_eq!(scene.frame(start + MS(800)).pill.map(|pill| pill.tone), Some(Tone::Working));
    }

    #[test]
    fn reduced_motion_only_wakes_for_deadlines() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.begin(RECT, STILL, Driving::Foreground, start);
        scene.focus_field(Rect::new(10, 10, 100, 20), STILL, start);
        assert_eq!(scene.next_wake(start), None);
        scene.click(Point::new(5, 5), 1, STILL, start);
        assert_eq!(scene.next_wake(start + MS(20)), Some(STILL_LIFE.saturating_sub(MS(20))));
        scene.end(start + MS(50));
        scene.advance(start + MS(400));
        assert!(scene.is_empty());
    }

    #[test]
    fn recording_keeps_its_start_when_moved_and_ends_on_stop() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.recording(RECT, ANIMATED, start);
        scene.recording(Rect::new(0, 0, 500, 500), ANIMATED, start + MS(5_000));
        let frame = scene.frame(start + MS(12_000)).recording.unwrap();
        assert_eq!((frame.seconds, frame.rect), (12, Rect::new(0, 0, 500, 500)));
        assert_eq!(scene.next_wake(start + MS(12_000)), Some(RECORDING_FRAME));
        scene.recording_stopped();
        assert!(scene.is_empty());
    }

    #[test]
    fn pointer_actions_carry_the_badge_and_it_fades_after_a_pause() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.glide(Point::new(0, 0), Point::new(300, 0), MS(200), ANIMATED, start);
        assert_eq!(scene.frame(start + MS(100)).badge.map(|badge| badge.at), Some(Point::new(150, 0)));
        scene.click(Point::new(300, 0), 1, ANIMATED, start + MS(200));
        scene.advance(start + MS(1300));
        assert!(scene.frame(start + MS(1300)).badge.is_some());
        scene.advance(start + MS(1400));
        scene.advance(start + MS(1750));
        assert!(scene.frame(start + MS(1750)).badge.is_none());
    }

    #[test]
    fn scans_are_debounced_rate_limited_and_never_shown_while_acting() {
        let start = Instant::now();
        let mut scene = Scene::default();
        assert!(scene.scan(RECT, ANIMATED, start));
        assert!(!scene.scan(RECT, ANIMATED, start + MS(100)));
        assert!(!scene.scan(RECT, ANIMATED, start + MS(1000)));
        assert!(scene.scan(Rect::new(0, 0, 300, 300), ANIMATED, start + MS(1000)));
        assert!(scene.scan(RECT, ANIMATED, start + MS(1600)));
        scene.begin(Rect::new(5, 5, 400, 400), ANIMATED, Driving::Foreground, start + MS(2000));
        assert!(!scene.scan(Rect::new(900, 0, 300, 300), ANIMATED, start + MS(4000)));
    }

    #[test]
    fn keystroke_chips_replace_each_other() {
        let start = Instant::now();
        let mut scene = Scene::default();
        let enter = ChipText::keys(vec![vec!["Enter".to_string()]]);
        scene.keystrokes(RECT, ChipText::typing(), ANIMATED, start);
        scene.keystrokes(RECT, enter.clone(), ANIMATED, start + MS(10));
        assert_eq!(scene.frame(start + MS(20)).chips.map(|chips| chips.text.clone()), Some(enter));
        scene.advance(start + MS(1200));
        assert!(scene.is_empty());
    }

    #[test]
    fn a_screenshot_shows_the_viewfinder_briefly() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.screenshot(RECT, ANIMATED, start);
        assert_eq!(scene.frame(start + MS(100)).viewfinder.map(|viewfinder| viewfinder.rect), Some(RECT));
        assert_eq!(scene.next_wake(start), Some(FRAME));
        scene.advance(start + MS(420));
        assert!(scene.is_empty());
    }
}
