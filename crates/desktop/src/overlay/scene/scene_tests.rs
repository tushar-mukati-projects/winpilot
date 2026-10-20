use super::*;
use crate::overlay::{
    presence::{FRAME, Tone},
    ripple::STILL_LIFE,
    scene_frame::Frame,
    scene_items::RECORDING_FRAME,
};

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
