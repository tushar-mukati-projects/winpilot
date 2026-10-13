use std::time::Duration;

use deckhand_core::{Point, Rect, WindowInfo, WindowState};
use deckhand_desktop::overlay::{Cue, OutOfSight};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyChips {
    Typing,
    Keys(Vec<Vec<String>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Driven,
    Watched,
    Aside(OutOfSight),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Staged {
    pub(crate) window: WindowInfo,
    pub(crate) view: View,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Effect {
    Click(Point, u8),
    Glide(Point, Point, Duration),
    Drag(Point, Point, Duration),
    Scroll(Option<Point>),
    Focus(Rect),
    Chips(Rect, KeyChips),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Draw {
    Click(Point, u8),
    Ripple(Point, u8),
    Glide(Point, Point, Duration),
    Focus(Rect),
    Chips(Rect, KeyChips),
    Pill(OutOfSight, Cue),
}

impl Effect {
    pub(crate) fn spot(&self) -> Option<Point> {
        match self {
            Self::Click(point, _) | Self::Drag(point, _, _) => Some(*point),
            Self::Scroll(point) => *point,
            Self::Focus(rect) => Some(rect.center()),
            Self::Glide(..) | Self::Chips(..) => None,
        }
    }
}

pub(crate) fn out_of_sight(window: &WindowInfo) -> Option<OutOfSight> {
    if !window.on_current_desktop {
        return Some(OutOfSight::OnDesktop(window.desktop));
    }
    (window.state == WindowState::Minimized).then_some(OutOfSight::Minimized)
}

pub(crate) fn view(background: bool, window: &WindowInfo, uncovered: impl FnOnce() -> bool) -> View {
    if !background && window.on_current_desktop {
        return View::Driven;
    }
    match out_of_sight(window) {
        Some(reason) => View::Aside(reason),
        None if uncovered() => View::Watched,
        None => View::Aside(OutOfSight::Behind),
    }
}

pub(crate) fn draws(view: View, effect: Effect) -> Vec<Draw> {
    match view {
        View::Driven => driven(effect),
        View::Watched => watched(effect),
        View::Aside(reason) => aside(effect).map(|cue| Draw::Pill(reason, cue)).into_iter().collect(),
    }
}

fn driven(effect: Effect) -> Vec<Draw> {
    match effect {
        Effect::Click(point, count) => vec![Draw::Click(point, count)],
        Effect::Glide(from, to, duration) | Effect::Drag(from, to, duration) => vec![Draw::Glide(from, to, duration)],
        Effect::Scroll(point) => point.map(|point| Draw::Click(point, 1)).into_iter().collect(),
        Effect::Focus(rect) => vec![Draw::Focus(rect)],
        Effect::Chips(rect, chips) => vec![Draw::Chips(rect, chips)],
    }
}

fn watched(effect: Effect) -> Vec<Draw> {
    match effect {
        Effect::Click(point, count) => vec![Draw::Ripple(point, count)],
        Effect::Glide(..) => Vec::new(),
        Effect::Drag(from, to, _) => vec![Draw::Ripple(from, 1), Draw::Ripple(to, 1)],
        Effect::Scroll(point) => point.map(|point| Draw::Ripple(point, 1)).into_iter().collect(),
        Effect::Focus(rect) => vec![Draw::Focus(rect)],
        Effect::Chips(rect, chips) => vec![Draw::Chips(rect, chips)],
    }
}

fn aside(effect: Effect) -> Option<Cue> {
    match effect {
        Effect::Click(_, count) => Some(Cue::Click(count)),
        Effect::Glide(..) | Effect::Focus(_) => None,
        Effect::Drag(..) => Some(Cue::Drag),
        Effect::Scroll(_) => Some(Cue::Scroll),
        Effect::Chips(_, KeyChips::Typing) => Some(Cue::Typing),
        Effect::Chips(_, KeyChips::Keys(chords)) => Some(Cue::Keys(chords)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::window;

    const AT: Point = Point::new(50, 60);
    const TO: Point = Point::new(90, 60);
    const FIELD: Rect = Rect::new(40, 50, 100, 20);
    const FRAME: Rect = Rect::new(0, 0, 400, 300);
    const GLIDE: Duration = Duration::from_millis(120);
    const BEHIND: View = View::Aside(OutOfSight::Behind);

    fn here() -> WindowInfo {
        window(5, "CapCut.exe", "CapCut", FRAME)
    }

    fn enter() -> KeyChips {
        KeyChips::Keys(vec![vec!["Enter".to_string()]])
    }

    #[test]
    fn foreground_calls_on_this_desktop_are_driven_without_a_visibility_check() {
        let view = view(false, &here(), || panic!("a foreground step needs no visibility check"));
        assert_eq!(view, View::Driven);
        let minimized = WindowInfo { state: WindowState::Minimized, ..here() };
        assert_eq!(super::view(false, &minimized, || false), View::Driven);
    }

    #[test]
    fn background_steps_are_watched_only_where_the_user_can_see_the_spot() {
        assert_eq!(view(true, &here(), || true), View::Watched);
        assert_eq!(view(true, &here(), || false), BEHIND);
        let minimized = WindowInfo { state: WindowState::Minimized, ..here() };
        assert_eq!(view(true, &minimized, || true), View::Aside(OutOfSight::Minimized));
    }

    #[test]
    fn other_desktops_are_always_aside_even_for_foreground_calls() {
        let elsewhere = WindowInfo { desktop: Some(2), on_current_desktop: false, ..here() };
        assert_eq!(view(false, &elsewhere, || true), View::Aside(OutOfSight::OnDesktop(Some(2))));
        assert_eq!(view(true, &elsewhere, || true), View::Aside(OutOfSight::OnDesktop(Some(2))));
        let unknown = WindowInfo { desktop: None, ..elsewhere };
        assert_eq!(out_of_sight(&unknown), Some(OutOfSight::OnDesktop(None)));
        assert_eq!(out_of_sight(&here()), None);
    }

    #[test]
    fn driven_steps_draw_everything_as_before() {
        assert_eq!(draws(View::Driven, Effect::Click(AT, 2)), [Draw::Click(AT, 2)]);
        assert_eq!(draws(View::Driven, Effect::Glide(AT, TO, GLIDE)), [Draw::Glide(AT, TO, GLIDE)]);
        assert_eq!(draws(View::Driven, Effect::Drag(AT, TO, GLIDE)), [Draw::Glide(AT, TO, GLIDE)]);
        assert_eq!(draws(View::Driven, Effect::Scroll(Some(AT))), [Draw::Click(AT, 1)]);
        assert!(draws(View::Driven, Effect::Scroll(None)).is_empty());
        assert_eq!(draws(View::Driven, Effect::Focus(FIELD)), [Draw::Focus(FIELD)]);
        assert_eq!(draws(View::Driven, Effect::Chips(FRAME, enter())), [Draw::Chips(FRAME, enter())]);
    }

    #[test]
    fn watched_steps_draw_on_the_window_without_cursor_glide_or_badge() {
        assert_eq!(draws(View::Watched, Effect::Click(AT, 2)), [Draw::Ripple(AT, 2)]);
        assert!(draws(View::Watched, Effect::Glide(AT, TO, GLIDE)).is_empty());
        assert_eq!(draws(View::Watched, Effect::Drag(AT, TO, GLIDE)), [Draw::Ripple(AT, 1), Draw::Ripple(TO, 1)]);
        assert_eq!(draws(View::Watched, Effect::Scroll(Some(AT))), [Draw::Ripple(AT, 1)]);
        assert!(draws(View::Watched, Effect::Scroll(None)).is_empty());
        assert_eq!(draws(View::Watched, Effect::Focus(FIELD)), [Draw::Focus(FIELD)]);
        assert_eq!(
            draws(View::Watched, Effect::Chips(FRAME, KeyChips::Typing)),
            [Draw::Chips(FRAME, KeyChips::Typing)]
        );
    }

    #[test]
    fn hidden_steps_only_update_the_activity_pill() {
        let pill = |cue| vec![Draw::Pill(OutOfSight::Behind, cue)];
        assert_eq!(draws(BEHIND, Effect::Click(AT, 1)), pill(Cue::Click(1)));
        assert_eq!(draws(BEHIND, Effect::Drag(AT, TO, GLIDE)), pill(Cue::Drag));
        assert_eq!(draws(BEHIND, Effect::Scroll(None)), pill(Cue::Scroll));
        assert_eq!(draws(BEHIND, Effect::Scroll(Some(AT))), pill(Cue::Scroll));
        assert_eq!(draws(BEHIND, Effect::Chips(FRAME, KeyChips::Typing)), pill(Cue::Typing));
        assert_eq!(draws(BEHIND, Effect::Chips(FRAME, enter())), pill(Cue::Keys(vec![vec!["Enter".to_string()]])));
        assert!(draws(BEHIND, Effect::Focus(FIELD)).is_empty());
        assert!(draws(BEHIND, Effect::Glide(AT, TO, GLIDE)).is_empty());
        let minimized = View::Aside(OutOfSight::Minimized);
        assert_eq!(draws(minimized, Effect::Click(AT, 1)), [Draw::Pill(OutOfSight::Minimized, Cue::Click(1))]);
    }

    #[test]
    fn points_and_fields_are_the_spot_that_is_checked() {
        assert_eq!(Effect::Click(AT, 1).spot(), Some(AT));
        assert_eq!(Effect::Drag(AT, TO, GLIDE).spot(), Some(AT));
        assert_eq!(Effect::Scroll(Some(TO)).spot(), Some(TO));
        assert_eq!(Effect::Focus(FIELD).spot(), Some(FIELD.center()));
        assert_eq!(Effect::Scroll(None).spot(), None);
        assert_eq!(Effect::Chips(FRAME, KeyChips::Typing).spot(), None);
        assert_eq!(Effect::Glide(AT, TO, GLIDE).spot(), None);
    }
}
