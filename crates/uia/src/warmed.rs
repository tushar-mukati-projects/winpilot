use std::collections::HashSet;

use winpilot_core::WindowRef;

#[derive(Default)]
pub(crate) struct Warmed {
    windows: HashSet<WindowRef>,
}

impl Warmed {
    pub(crate) fn first_visit(&mut self, window: WindowRef) -> bool {
        self.windows.insert(window)
    }

    pub(crate) fn prune(&mut self, mut window_alive: impl FnMut(WindowRef) -> bool) {
        self.windows.retain(|window| window_alive(*window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_visit_of_a_window_counts() {
        let mut warmed = Warmed::default();
        assert!(warmed.first_visit(WindowRef(7)));
        assert!(!warmed.first_visit(WindowRef(7)));
        assert!(warmed.first_visit(WindowRef(8)));
        assert!(!warmed.first_visit(WindowRef(8)));
    }

    #[test]
    fn closed_windows_are_forgotten() {
        let mut warmed = Warmed::default();
        warmed.first_visit(WindowRef(7));
        warmed.first_visit(WindowRef(8));
        warmed.prune(|window| window != WindowRef(7));
        assert!(warmed.first_visit(WindowRef(7)));
        assert!(!warmed.first_visit(WindowRef(8)));
    }
}
