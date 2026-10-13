use winpilot_core::{
    Element, ElementRef, Error, ErrorCode, Point, Rect, Result, WindowInfo, WindowRef, format::element_line,
};

use super::{
    locate::{Located, not_visible},
    target::Prepared,
};
use crate::{cover::window_covers, engine::Engine, on_screen::visible_part};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HitRelation {
    same: bool,
    target_contains_hit: bool,
    hit_contains_target: bool,
    in_target_window: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitVerdict {
    OnTarget,
    CoveredByWindow,
    CoveredInWindow,
}

const fn judge_hit(relation: HitRelation) -> HitVerdict {
    if relation.same || relation.target_contains_hit || relation.hit_contains_target {
        return HitVerdict::OnTarget;
    }
    if relation.in_target_window { HitVerdict::CoveredInWindow } else { HitVerdict::CoveredByWindow }
}

fn quarter_points(rect: Rect) -> [Point; 4] {
    let left = rect.x.saturating_add(fraction(rect.width, 1));
    let right = rect.x.saturating_add(fraction(rect.width, 3));
    let top = rect.y.saturating_add(fraction(rect.height, 1));
    let bottom = rect.y.saturating_add(fraction(rect.height, 3));
    [Point::new(left, top), Point::new(right, top), Point::new(left, bottom), Point::new(right, bottom)]
}

fn fraction(length: i32, quarters: i64) -> i32 {
    i32::try_from(i64::from(length) * quarters / 4).unwrap_or(0)
}

fn candidate_points(first: Point, visible: Rect) -> Vec<Point> {
    let mut points = vec![first];
    for point in quarter_points(visible) {
        if !points.contains(&point) {
            points.push(point);
        }
    }
    points
}

pub(crate) fn hit_point(engine: &Engine, prepared: &Prepared, located: &Located) -> Result<Point> {
    match tracked_hit(prepared, located) {
        Some(element) => verify_hit(engine, element, located.point, &located.window),
        None => require_window_at(engine, located.point, &located.window).map(|()| located.point),
    }
}

pub(crate) fn confirm_hit(engine: &Engine, prepared: &Prepared, located: &Located, point: Point) -> Result<()> {
    match tracked_hit(prepared, located) {
        Some(element) => confirm_element_at(engine, element.id, located.window.hwnd, point),
        None => require_window_at(engine, point, &located.window),
    }
}

pub(crate) fn lands_in(engine: &Engine, point: Point, window: &WindowInfo) -> bool {
    let landed = engine.uia.run(move |uia| {
        let hit = uia.element_at(point)?;
        uia.window_of(hit.id)
    });
    landed.is_ok_and(|hwnd| !window_covers(engine, window.hwnd, hwnd))
}

fn tracked_hit<'a>(prepared: &Prepared, located: &'a Located) -> Option<&'a Element> {
    located.element.as_ref().filter(|_| prepared.tracked_element().is_some())
}

fn require_window_at(engine: &Engine, point: Point, window: &WindowInfo) -> Result<()> {
    if lands_in(engine, point, window) {
        return Ok(());
    }
    Err(window_covered(window.hwnd, point))
}

fn confirm_element_at(engine: &Engine, id: ElementRef, window: WindowRef, point: Point) -> Result<()> {
    match probe(engine, id, window, point)? {
        (HitVerdict::OnTarget, _) => Ok(()),
        (verdict, hit) => Err(covered(id, verdict, &hit)),
    }
}

fn verify_hit(engine: &Engine, element: &Element, first: Point, window: &WindowInfo) -> Result<Point> {
    let visible = visible_part(element, window).ok_or_else(|| not_visible(element.id))?;
    let mut covering = None;
    for point in candidate_points(first, visible) {
        match probe(engine, element.id, window.hwnd, point)? {
            (HitVerdict::OnTarget, _) => return Ok(point),
            found => covering = Some(found),
        }
    }
    Err(covering.map_or_else(|| not_visible(element.id), |(verdict, hit)| covered(element.id, verdict, &hit)))
}

fn probe(engine: &Engine, target: ElementRef, window: WindowRef, point: Point) -> Result<(HitVerdict, Element)> {
    engine.uia.run(move |uia| {
        let hit = uia.element_at(point)?;
        let same = hit.id == target;
        let target_contains_hit = !same && uia.contains(target, hit.id)?;
        let hit_contains_target = !same && !target_contains_hit && uia.contains(hit.id, target)?;
        let in_target_window = uia.window_of(hit.id).is_ok_and(|owner| owner == window);
        let relation = HitRelation { same, target_contains_hit, hit_contains_target, in_target_window };
        Ok((judge_hit(relation), hit))
    })
}

fn covered(id: ElementRef, verdict: HitVerdict, hit: &Element) -> Error {
    let covering = element_line(hit, false);
    let message = match verdict {
        HitVerdict::CoveredByWindow => format!("{id} is covered by another window's {covering}"),
        HitVerdict::CoveredInWindow | HitVerdict::OnTarget => format!("{id} is covered by {covering}"),
    };
    Error::new(ErrorCode::Obscured, message)
}

fn window_covered(hwnd: WindowRef, point: Point) -> Error {
    Error::new(ErrorCode::Obscured, format!("another window covers {hwnd} at {},{}", point.x, point.y))
}

#[cfg(test)]
mod tests {
    use winpilot_core::Role;

    use super::*;
    use crate::{
        resolve::Reach,
        test_fixture::{element, window},
    };

    fn located(element: Option<Element>) -> Located {
        Located { window: window(5, "app.exe", "App", Rect::new(0, 0, 100, 100)), element, point: Point::new(50, 50) }
    }

    #[test]
    fn only_tracked_elements_are_checked_element_by_element() {
        let button = element(1, 1, Role::Button, "OK");
        let tracked = Prepared { window: located(None).window, reach: Reach::Element(button.clone()) };
        assert!(tracked_hit(&tracked, &located(Some(button.clone()))).is_some());
        let at_point = Prepared { reach: Reach::Point { point: Point::new(50, 50), under: button.clone() }, ..tracked };
        assert!(tracked_hit(&at_point, &located(Some(button))).is_none());
        let whole = Prepared { reach: Reach::Window, ..at_point };
        assert!(tracked_hit(&whole, &located(None)).is_none());
    }

    #[test]
    fn a_covered_window_names_where_it_is_covered() {
        let error = window_covered(WindowRef(42), Point::new(5, 7));
        assert_eq!(error.code, ErrorCode::Obscured);
        assert_eq!(error.message, "another window covers w42 at 5,7");
    }

    #[test]
    fn quarter_points_sit_inside_the_rect() {
        let rect = Rect::new(100, 200, 40, 20);
        assert_eq!(
            quarter_points(rect),
            [Point::new(110, 205), Point::new(130, 205), Point::new(110, 215), Point::new(130, 215)]
        );
        assert!(quarter_points(rect).iter().all(|point| rect.contains(*point)));
    }

    #[test]
    fn quarter_points_of_a_tiny_rect_collapse_to_its_corner() {
        let rect = Rect::new(-5, -5, 1, 1);
        assert!(quarter_points(rect).iter().all(|point| *point == Point::new(-5, -5)));
    }

    #[test]
    fn candidates_start_with_the_aimed_point_and_skip_duplicates() {
        let rect = Rect::new(0, 0, 4, 4);
        let points = candidate_points(Point::new(1, 1), rect);
        assert_eq!(points[0], Point::new(1, 1));
        assert_eq!(points.len(), 4);
        let mut unique = points.clone();
        unique.dedup();
        assert_eq!(unique, points);
    }

    fn in_window(relation: HitRelation) -> HitRelation {
        HitRelation { in_target_window: true, ..relation }
    }

    #[test]
    fn the_target_itself_or_anything_inside_it_is_on_target() {
        assert_eq!(judge_hit(in_window(HitRelation { same: true, ..HitRelation::default() })), HitVerdict::OnTarget);
        let inside = HitRelation { target_contains_hit: true, ..HitRelation::default() };
        assert_eq!(judge_hit(in_window(inside)), HitVerdict::OnTarget);
    }

    #[test]
    fn a_same_window_ancestor_is_not_a_cover() {
        let ancestor = in_window(HitRelation { hit_contains_target: true, ..HitRelation::default() });
        assert_eq!(judge_hit(ancestor), HitVerdict::OnTarget);
    }

    #[test]
    fn related_elements_that_winpilot_files_under_another_window_still_reach_the_target() {
        let hosted = HitRelation { hit_contains_target: true, ..HitRelation::default() };
        assert_eq!(judge_hit(hosted), HitVerdict::OnTarget);
        assert_eq!(judge_hit(HitRelation { same: true, ..HitRelation::default() }), HitVerdict::OnTarget);
    }

    #[test]
    fn an_unrelated_element_of_the_same_window_covers_the_target() {
        assert_eq!(judge_hit(in_window(HitRelation::default())), HitVerdict::CoveredInWindow);
    }

    #[test]
    fn an_unrelated_element_of_another_window_covers_the_target() {
        assert_eq!(judge_hit(HitRelation::default()), HitVerdict::CoveredByWindow);
    }

    #[test]
    fn covered_names_the_covering_element_and_whose_window_it_is() {
        let hit = element(9, 0, Role::Button, "OK");
        let inside = covered(ElementRef(12), HitVerdict::CoveredInWindow, &hit);
        assert_eq!(inside.code, ErrorCode::Obscured);
        assert_eq!(inside.message, "e12 is covered by e9 button \"OK\"");
        let outside = covered(ElementRef(12), HitVerdict::CoveredByWindow, &hit);
        assert_eq!(outside.code, ErrorCode::Obscured);
        assert_eq!(outside.message, "e12 is covered by another window's e9 button \"OK\"");
        assert_eq!(not_visible(ElementRef(12)).message, "e12 is not visible on screen");
    }
}
