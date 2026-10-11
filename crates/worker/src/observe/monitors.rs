use deckhand_core::{
    Result,
    protocol::{MonitorInfo, WindowsReport},
};
use deckhand_desktop::Monitor;

pub fn list() -> Result<WindowsReport> {
    let monitors = deckhand_desktop::monitors()?.iter().map(monitor_info).collect();
    Ok(WindowsReport { monitors, ..WindowsReport::default() })
}

fn monitor_info(monitor: &Monitor) -> MonitorInfo {
    MonitorInfo {
        index: u32::try_from(monitor.index).unwrap_or(u32::MAX),
        rect: monitor.rect,
        work_area: monitor.work_area,
        scale_percent: monitor.scale_percent,
        primary: monitor.primary,
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Rect, format};

    use super::*;
    use crate::test_fixture::engine;

    #[test]
    fn desktop_monitors_convert_field_for_field() {
        let monitor = Monitor {
            index: 2,
            rect: Rect::new(-1920, 0, 1920, 1080),
            work_area: Rect::new(-1920, 0, 1920, 1032),
            scale_percent: 150,
            primary: false,
        };
        let info = monitor_info(&monitor);
        assert_eq!(
            info,
            MonitorInfo {
                index: 2,
                rect: Rect::new(-1920, 0, 1920, 1080),
                work_area: Rect::new(-1920, 0, 1920, 1032),
                scale_percent: 150,
                primary: false
            }
        );
        assert_eq!(monitor_info(&Monitor { index: usize::MAX, ..monitor }).index, u32::MAX);
    }

    #[test]
    fn live_monitors_use_the_screenshot_order() {
        let report = list().unwrap();
        let captured = engine().capturer.monitor_rects().unwrap();
        let listed: Vec<Rect> = report.monitors.iter().map(|monitor| monitor.rect).collect();
        assert_eq!(listed, captured);
        assert!(report.monitors[0].primary);
        assert!(report.monitors.iter().enumerate().all(|(index, monitor)| monitor.index as usize == index));
        assert!(report.monitors.iter().all(|monitor| monitor.rect.intersection(monitor.work_area).is_some()));
        assert!(report.windows.is_empty() && report.apps.is_empty());
        let text = format::windows(&report);
        assert_eq!(text.lines().count(), report.monitors.len());
        assert!(text.lines().next().unwrap().starts_with("monitor 0 "));
    }
}
