use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Point, Rect};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, POINT, RECT},
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            Threading::{GetCurrentProcessId, GetCurrentThread, OpenThread, THREAD_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Accessibility::{CUIAutomation, IUIAutomation},
            WindowsAndMessaging::{
                FindWindowExW, GA_ROOT, GetAncestor, GetClassNameW, GetWindowRect, IsWindowVisible, WindowFromPoint,
            },
        },
    },
    core::w,
};

use super::{
    client::Overlay,
    monitor_at::{primary_work_area, scale_at},
    options::OverlayOptions,
};
use crate::{com::ComApartment, dpi::enable_dpi_awareness};

const SETTLE: Duration = Duration::from_millis(350);

fn root_at(point: Point) -> HWND {
    let hwnd = unsafe { WindowFromPoint(POINT { x: point.x, y: point.y }) };
    unsafe { GetAncestor(hwnd, GA_ROOT) }
}

fn class_of(hwnd: HWND) -> String {
    let mut name = [0u16; 128];
    let length = unsafe { GetClassNameW(hwnd, &mut name) };
    String::from_utf16_lossy(&name[..usize::try_from(length).unwrap_or(0)])
}

fn visible_overlay_covers(point: Point) -> bool {
    let mut after: Option<HWND> = None;
    loop {
        let Ok(found) = (unsafe { FindWindowExW(None, after, w!("DeckhandOverlay"), None) }) else { return false };
        let mut rect = RECT::default();
        let placed = unsafe { GetWindowRect(found, &raw mut rect) }.is_ok();
        let inside = rect.left <= point.x && point.x < rect.right && rect.top <= point.y && point.y < rect.bottom;
        if placed && inside && unsafe { IsWindowVisible(found) }.as_bool() {
            return true;
        }
        after = Some(found);
    }
}

fn uia_owner_at(point: Point) -> (u32, String) {
    let _apartment = ComApartment::enter_mta().unwrap();
    let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let element = unsafe { automation.ElementFromPoint(POINT { x: point.x, y: point.y }) }.unwrap();
    let process = unsafe { element.CurrentProcessId() }.unwrap();
    let class = unsafe { element.CurrentClassName() }.map(|name| name.to_string()).unwrap_or_default();
    (process.cast_unsigned(), class)
}

struct ThreadCycles(HANDLE);

impl ThreadCycles {
    fn open(thread_id: u32) -> Self {
        Self(unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, thread_id) }.unwrap())
    }

    fn now(&self) -> u64 {
        cycles_of(self.0)
    }
}

impl Drop for ThreadCycles {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

windows_core::link!("kernel32.dll" "system" fn QueryThreadCycleTime(thread: HANDLE, cycles: *mut u64) -> windows_core::BOOL);

fn cycles_of(thread: HANDLE) -> u64 {
    let mut cycles = 0u64;
    assert!(unsafe { QueryThreadCycleTime(thread, &raw mut cycles) }.as_bool());
    cycles
}

fn cycles_per_second() -> f64 {
    let own = unsafe { GetCurrentThread() };
    let spin = || {
        let (before, started) = (cycles_of(own), Instant::now());
        while started.elapsed() < Duration::from_millis(60) {
            std::hint::spin_loop();
        }
        (cycles_of(own) - before) as f64 / started.elapsed().as_secs_f64()
    };
    (0..6).map(|_| spin()).fold(0.0, f64::max)
}

fn share_of_one_core(overlay: &Overlay, rate: f64, wall: Duration) -> f64 {
    let thread = ThreadCycles::open(overlay.thread_id());
    let (before, started) = (thread.now(), Instant::now());
    thread::sleep(wall);
    (thread.now() - before) as f64 / rate / started.elapsed().as_secs_f64() * 100.0
}

fn probe_and_window() -> (Point, Rect) {
    enable_dpi_awareness().unwrap();
    let area = primary_work_area().unwrap();
    let probe = Point::new(area.center().x, area.y + area.height / 3);
    let scale = scale_at(probe);
    let pill_middle = (22.0 * scale).round() as i32;
    let rect = Rect::new(probe.x - 450, probe.y - pill_middle, 900, area.height / 2);
    (probe, rect)
}

#[test]
fn overlay_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Overlay>();
}

#[test]
#[ignore = "shows the overlay"]
fn hit_tests_and_clicks_pass_through_the_overlay() {
    let (probe, rect) = probe_and_window();
    let border_point = Point::new(rect.x - 1, probe.y + 60);
    let below = (root_at(probe), root_at(border_point));
    let overlay = Overlay::spawn(OverlayOptions::default()).unwrap();
    assert!(!overlay.hidden_for_capture().is_hiding());
    overlay.begin(rect);
    overlay.click(Point::new(probe.x + 200, probe.y + 100), 1);
    thread::sleep(SETTLE);
    assert!(visible_overlay_covers(probe), "the pill should be over the probe point");
    assert!(visible_overlay_covers(border_point), "the border should be over the border point");
    for (point, expected) in [(probe, below.0), (border_point, below.1)] {
        assert_eq!(root_at(point), expected);
        assert_ne!(class_of(root_at(point)), "DeckhandOverlay");
        let (process, class) = uia_owner_at(point);
        assert_ne!(process, unsafe { GetCurrentProcessId() });
        assert_ne!(class, "DeckhandOverlay");
    }
    overlay.end();
    thread::sleep(Duration::from_millis(300));
}

#[test]
#[ignore = "shows the overlay"]
fn animating_costs_little_and_idle_costs_nothing() {
    let (_, rect) = probe_and_window();
    let area = primary_work_area().unwrap();
    let rate = cycles_per_second();
    let overlay = Overlay::spawn(OverlayOptions::default()).unwrap();
    overlay.begin(rect);
    thread::sleep(Duration::from_millis(600));
    let window = share_of_one_core(&overlay, rate, Duration::from_secs(2));
    overlay.begin(area);
    thread::sleep(Duration::from_millis(600));
    let full = share_of_one_core(&overlay, rate, Duration::from_secs(2));
    overlay.end();
    thread::sleep(Duration::from_millis(400));
    let idle = share_of_one_core(&overlay, rate, Duration::from_millis(1500));
    eprintln!(
        "timing: overlay animating {window:.2}% of one core on {}x{}, {full:.2}% on the {}x{} work area, idle {idle:.3}%",
        rect.width, rect.height, area.width, area.height
    );
    assert!(idle < 0.2, "idle overlay used {idle:.3}% of a core");
    let limit = if cfg!(debug_assertions) { 15.0 } else { 3.0 };
    assert!(full < limit, "animating overlay used {full:.2}% of a core");
}

#[test]
#[ignore = "shows the overlay"]
fn every_effect_runs_and_shuts_down_cleanly() {
    let (probe, rect) = probe_and_window();
    let overlay = Overlay::spawn(OverlayOptions {
        kill_hotkey: Some("ctrl+alt+escape".to_string()),
        ..OverlayOptions::default()
    })
    .unwrap();
    overlay.begin(rect);
    overlay.glide(Point::new(probe.x - 300, probe.y + 200), probe, Duration::from_millis(200));
    overlay.click(probe, 2);
    overlay.screenshot(rect);
    overlay.recording(rect);
    thread::sleep(Duration::from_millis(300));
    overlay.begin(Rect::new(rect.x + 100, rect.y + 40, rect.width - 200, rect.height - 80));
    overlay.stopped();
    thread::sleep(Duration::from_millis(400));
    overlay.resumed();
    overlay.recording_stopped();
    let started = Instant::now();
    drop(overlay);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
#[ignore = "shows the overlay"]
fn demo_mode_hides_the_overlay_while_capturing() {
    let (_, rect) = probe_and_window();
    let excluded = Overlay::spawn(OverlayOptions::default()).unwrap();
    assert!(!excluded.hidden_for_capture().is_hiding());
    let demo = Overlay::spawn(OverlayOptions { capturable: true, ..OverlayOptions::default() }).unwrap();
    let idle = Instant::now();
    let nothing_shown = demo.hidden_for_capture();
    assert!(nothing_shown.is_hiding() && nothing_shown.is_confirmed());
    eprintln!("timing: concealing an empty overlay took {:.1} ms", idle.elapsed().as_secs_f64() * 1000.0);
    drop(nothing_shown);
    demo.begin(rect);
    thread::sleep(SETTLE);
    let pill = Point::new(rect.center().x, rect.y + 20);
    assert!(visible_overlay_covers(pill));
    let started = Instant::now();
    let guard = demo.hidden_for_capture();
    assert!(guard.is_hiding() && guard.is_confirmed() && started.elapsed() < Duration::from_millis(250));
    assert!(!visible_overlay_covers(pill));
    drop(guard);
    thread::sleep(Duration::from_millis(100));
    assert!(visible_overlay_covers(pill));
    demo.end();
}
