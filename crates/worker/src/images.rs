use std::collections::VecDeque;

use winpilot_core::{
    Error, ErrorCode, ImageMapping, ImageRef, Point, Result, TargetSpec, WindowRef, target::PointSpec,
};

const CAPACITY: usize = 32;
const COVERED: &str = "that point is covered by another window now; take a new screenshot";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ImageRecord {
    pub(crate) mapping: ImageMapping,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) window: Option<WindowRef>,
}

#[derive(Debug)]
pub(crate) struct ImageStore {
    first: u32,
    next: u32,
    entries: VecDeque<(ImageRef, ImageRecord)>,
}

impl Default for ImageStore {
    fn default() -> Self {
        Self::starting_at(1)
    }
}

impl ImageStore {
    pub(crate) fn starting_at(first: u32) -> Self {
        Self { first, next: first.saturating_sub(1), entries: VecDeque::with_capacity(CAPACITY) }
    }

    pub(crate) fn insert(&mut self, record: ImageRecord) -> ImageRef {
        self.next = self.next.saturating_add(1);
        let image = ImageRef(self.next);
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((image, record));
        image
    }

    pub(crate) fn record(&self, image: ImageRef) -> Result<ImageRecord> {
        if image.0 < self.first {
            return Err(Error::not_found(format!(
                "{image} came from a previous worker (it restarted); take a new screenshot"
            )));
        }
        self.entries
            .iter()
            .find(|(known, _)| *known == image)
            .map(|(_, record)| *record)
            .ok_or_else(|| Error::not_found(format!("{image} is unknown or too old; take a new screenshot")))
    }

    pub(crate) fn latest(&self) -> Option<(ImageRef, ImageRecord)> {
        self.entries.back().copied()
    }

    pub(crate) fn bare_point_note(&self, target: &TargetSpec) -> Option<String> {
        let TargetSpec::Point(PointSpec { x, y, img: None }) = target else { return None };
        let (image, record) = self.latest()?;
        record.bare_point_note(image, Point::new(*x, *y))
    }
}

impl ImageRecord {
    pub(crate) fn screen_point(&self, image: ImageRef, point: Point) -> Result<Point> {
        if !self.holds(point) {
            return Err(Error::invalid(format!(
                "point {},{} is outside {image} ({}x{})",
                point.x, point.y, self.width, self.height
            )));
        }
        Ok(self.mapping.to_screen(point))
    }

    pub(crate) fn check_hit_window(&self, hit: WindowRef, covers: impl FnOnce(WindowRef) -> bool) -> Result<()> {
        match self.window {
            Some(source) if source != hit && covers(source) => Err(Error::new(ErrorCode::Obscured, COVERED)),
            _ => Ok(()),
        }
    }

    pub(crate) fn bare_point_note(&self, image: ImageRef, point: Point) -> Option<String> {
        let scale = self.mapping.scale;
        (scale > 1.0 && self.holds(point)).then(|| {
            format!(
                "{},{} were used as screen pixels; screenshot {image} has scale {scale:.2}, so add \"img\":\"{image}\" if you measured them there",
                point.x, point.y
            )
        })
    }

    fn holds(&self, point: Point) -> bool {
        let inside = |value: i32, limit: u32| u32::try_from(value).is_ok_and(|value| value < limit);
        inside(point.x, self.width) && inside(point.y, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(scale: f64) -> ImageRecord {
        ImageRecord {
            mapping: ImageMapping { origin: Point::new(100, 50), scale },
            width: 1280,
            height: 800,
            window: None,
        }
    }

    #[test]
    fn remembers_recent_images() {
        let mut store = ImageStore::default();
        let first = store.insert(record(1.0));
        let second = store.insert(record(2.0));
        assert_eq!(store.record(first).unwrap(), record(1.0));
        assert_eq!(store.record(second).unwrap(), record(2.0));
        assert_eq!(store.latest(), Some((second, record(2.0))));
        assert_eq!(ImageStore::default().latest(), None);
    }

    #[test]
    fn bare_points_inside_a_scaled_shot_get_a_note() {
        let sized = |scale| ImageRecord { width: 640, height: 360, ..record(scale) };
        assert_eq!(
            sized(2.0).bare_point_note(ImageRef(7), Point::new(300, 200)).as_deref(),
            Some(
                "300,200 were used as screen pixels; screenshot i7 has scale 2.00, so add \"img\":\"i7\" if you measured them there"
            )
        );
        assert_eq!(sized(2.0).bare_point_note(ImageRef(7), Point::new(900, 200)), None);
        assert_eq!(sized(1.0).bare_point_note(ImageRef(7), Point::new(300, 200)), None);
    }

    #[test]
    fn refs_from_a_previous_worker_are_explained() {
        let mut store = ImageStore::starting_at(100_000_001);
        let current = store.insert(record(1.0));
        assert_eq!(current, ImageRef(100_000_001));
        let old = store.record(ImageRef(3)).unwrap_err();
        assert!(old.message.contains("previous worker"), "{}", old.message);
    }

    #[test]
    fn forgets_the_oldest_beyond_capacity() {
        let mut store = ImageStore::default();
        let first = store.insert(record(1.0));
        for _ in 0..CAPACITY {
            store.insert(record(2.0));
        }
        assert!(store.record(first).is_err());
    }

    #[test]
    fn points_inside_the_image_map_to_the_screen() {
        let image = record(2.0);
        assert_eq!(image.screen_point(ImageRef(3), Point::new(0, 0)).unwrap(), Point::new(100, 50));
        assert_eq!(image.screen_point(ImageRef(3), Point::new(1279, 799)).unwrap(), Point::new(2658, 1648));
    }

    #[test]
    fn points_outside_the_image_are_refused() {
        let image = record(1.0);
        for point in [Point::new(1280, 10), Point::new(10, 800), Point::new(-1, 0), Point::new(0, -5)] {
            let error = image.screen_point(ImageRef(3), point).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument);
        }
        let error = image.screen_point(ImageRef(3), Point::new(1280, 10)).unwrap_err();
        assert_eq!(error.message, "point 1280,10 is outside i3 (1280x800)");
    }

    #[test]
    fn window_shots_insist_on_their_window() {
        let shot = ImageRecord { window: Some(WindowRef(7)), ..record(1.0) };
        let covering = |_| true;
        assert!(shot.check_hit_window(WindowRef(7), |_| panic!("asked about its own window")).is_ok());
        let error = shot.check_hit_window(WindowRef(8), covering).unwrap_err();
        assert_eq!(error.code, ErrorCode::Obscured);
        assert_eq!(error.message, COVERED);
        assert!(record(1.0).check_hit_window(WindowRef(8), covering).is_ok());
    }

    #[test]
    fn windows_that_belong_to_the_shot_window_are_not_covers() {
        let shot = ImageRecord { window: Some(WindowRef(7)), ..record(1.0) };
        let mut asked = None;
        let flyout = shot.check_hit_window(WindowRef(9), |source| {
            asked = Some(source);
            false
        });
        assert!(flyout.is_ok());
        assert_eq!(asked, Some(WindowRef(7)));
    }
}
