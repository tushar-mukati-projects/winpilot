use deckhand_core::{Point, Rect};

use super::scale::scaled;

const CORE_WIDTH: f32 = 2.0;
const BLOOM_WIDTH: f32 = 24.0;
const TIGHT_SPREAD: f32 = 3.4;
const WIDE_SPREAD: f32 = 10.5;
const MINIMIZED_COORDINATE: i32 = -32_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BorderMetrics {
    pub(crate) core: f32,
    pub(crate) bloom: f32,
    pub(crate) band: i32,
    scale: f32,
}

impl BorderMetrics {
    pub(crate) fn at_scale(scale: f32) -> Self {
        let core = scaled(CORE_WIDTH, scale).round().max(CORE_WIDTH);
        let bloom = scaled(BLOOM_WIDTH, scale).round();
        Self { core, bloom, band: (core + bloom).ceil() as i32, scale: scaled(1.0, scale) }
    }

    pub(crate) fn tight_spread(&self) -> f32 {
        TIGHT_SPREAD * self.scale
    }

    pub(crate) fn wide_spread(&self) -> f32 {
        WIDE_SPREAD * self.scale
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenSides {
    pub(crate) left: bool,
    pub(crate) top: bool,
    pub(crate) right: bool,
    pub(crate) bottom: bool,
}

pub(crate) fn fit_on_screen(rect: Rect, band: i32, open: OpenSides) -> Rect {
    let inset = |open: bool| if open { 0 } else { band.max(0) };
    let left = rect.x.saturating_add(inset(open.left));
    let top = rect.y.saturating_add(inset(open.top));
    let right = rect.right().saturating_sub(inset(open.right));
    let bottom = rect.bottom().saturating_sub(inset(open.bottom));
    let fitted = Rect::from_edges(left, top, right, bottom);
    let roomy = fitted.width >= band.saturating_mul(2) && fitted.height >= band.saturating_mul(2);
    if roomy { fitted } else { rect }
}

pub(crate) fn is_framable(target: Rect) -> bool {
    let minimized = target.x <= MINIMIZED_COORDINATE && target.y <= MINIMIZED_COORDINATE;
    !target.is_empty() && !minimized
}

pub(crate) fn sides_open_within(rect: Rect, band: i32, monitor: Rect) -> OpenSides {
    let [left, top, right, bottom] = outside_probes(rect, band).map(|(x, y)| monitor.contains(Point::new(x, y)));
    OpenSides { left, top, right, bottom }
}

pub(crate) fn outside_probes(rect: Rect, band: i32) -> [(i32, i32); 4] {
    let reach = (band / 2).max(1);
    let center = rect.center();
    [
        (rect.x.saturating_sub(reach), center.y),
        (center.x, rect.y.saturating_sub(reach)),
        (rect.right().saturating_add(reach - 1), center.y),
        (center.x, rect.bottom().saturating_add(reach - 1)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: OpenSides = OpenSides { left: true, top: true, right: true, bottom: true };

    #[test]
    fn metrics_scale_with_dpi() {
        let normal = BorderMetrics::at_scale(1.0);
        assert_eq!((normal.core, normal.bloom, normal.band), (2.0, 24.0, 26));
        let large = BorderMetrics::at_scale(1.5);
        assert_eq!((large.core, large.bloom, large.band), (3.0, 36.0, 39));
        assert!((large.wide_spread() - 1.5 * normal.wide_spread()).abs() < 1e-4);
    }

    #[test]
    fn closed_sides_pull_the_frame_inside_the_screen() {
        let maximized = Rect::new(0, 0, 1920, 1032);
        let open = OpenSides { left: false, top: false, right: false, bottom: true };
        assert_eq!(fit_on_screen(maximized, 12, open), Rect::new(12, 12, 1896, 1020));
        assert_eq!(fit_on_screen(maximized, 12, ALL), maximized);
    }

    #[test]
    fn tiny_rects_are_left_alone() {
        let closed = OpenSides { left: false, top: false, right: false, bottom: false };
        assert_eq!(fit_on_screen(Rect::new(0, 0, 30, 30), 12, closed), Rect::new(0, 0, 30, 30));
    }

    #[test]
    fn only_sides_with_room_on_the_windows_own_monitor_are_open() {
        let monitor = Rect::new(0, 0, 1920, 1080);
        let floating = Rect::new(400, 300, 800, 500);
        assert_eq!(sides_open_within(floating, 26, monitor), ALL);
        let at_the_seam = Rect::new(1200, 300, 720, 500);
        let open = sides_open_within(at_the_seam, 26, monitor);
        assert_eq!(open, OpenSides { left: true, top: true, right: false, bottom: true });
        assert_eq!(fit_on_screen(at_the_seam, 26, open).right(), at_the_seam.right() - 26);
        let maximized = Rect::new(0, 0, 1920, 1040);
        assert_eq!(
            sides_open_within(maximized, 26, monitor),
            OpenSides { left: false, top: false, right: false, bottom: true }
        );
    }

    #[test]
    fn minimized_and_empty_targets_are_not_framed() {
        assert!(is_framable(Rect::new(100, 100, 800, 600)));
        assert!(is_framable(Rect::new(-1920, -200, 800, 600)));
        assert!(!is_framable(Rect::new(-32_000, -32_000, 160, 28)));
        assert!(!is_framable(Rect::new(100, 100, 0, 600)));
        assert!(!is_framable(Rect::default()));
    }

    #[test]
    fn probes_sit_just_outside_each_side() {
        let probes = outside_probes(Rect::new(100, 100, 200, 100), 12);
        assert_eq!(probes, [(94, 150), (200, 94), (305, 150), (200, 205)]);
    }
}
