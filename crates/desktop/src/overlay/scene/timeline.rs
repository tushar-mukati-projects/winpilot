use std::time::{Duration, Instant};

use super::Scene;
use crate::overlay::{presence::FRAME, scene_frame::Frame};

impl Scene {
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
}
