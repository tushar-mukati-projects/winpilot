mod key_plans;
mod mouse_plans;

use std::time::Duration;

pub(crate) use key_plans::{Toolkit, key_code, keys_plan, text_plan};
pub(crate) use mouse_plans::{click_plan, drag_plan, release_plan, scroll_plan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sent {
    Message { id: u32, wparam: usize, lparam: isize },
    Pause(Duration),
}

const fn message(id: u32, wparam: usize, lparam: isize) -> Sent {
    Sent::Message { id, wparam, lparam }
}

#[cfg(test)]
fn ids(plan: &[Sent]) -> Vec<u32> {
    plan.iter()
        .filter_map(|sent| match sent {
            Sent::Message { id, .. } => Some(*id),
            Sent::Pause(_) => None,
        })
        .collect()
}

#[cfg(test)]
fn messages(plan: &[Sent]) -> Vec<(u32, usize, isize)> {
    plan.iter()
        .filter_map(|sent| match *sent {
            Sent::Message { id, wparam, lparam } => Some((id, wparam, lparam)),
            Sent::Pause(_) => None,
        })
        .collect()
}
