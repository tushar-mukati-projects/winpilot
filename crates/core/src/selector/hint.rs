use super::{Selector, term::Term};
use crate::role::Role;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHint {
    pub role: Option<Role>,
    pub name: String,
    pub exact: bool,
}

impl Selector {
    pub fn search_hint(&self) -> Option<SearchHint> {
        let terms = &self.steps.iter().rev().find(|step| step.filters_elements())?.terms;
        let (name, exact) = terms.iter().find_map(|term| match term {
            Term::Name(name) => Some((name, true)),
            Term::NameContains(name) | Term::Text(name) => Some((name, false)),
            _ => None,
        })?;
        let role = terms.iter().find_map(|term| match term {
            Term::Role(role) => Some(*role),
            _ => None,
        });
        Some(SearchHint { role, name: name.clone(), exact })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(text: &str) -> Option<SearchHint> {
        text.parse::<Selector>().unwrap().search_hint()
    }

    #[test]
    fn the_last_element_step_gives_the_role_and_name_to_search_for() {
        let save = SearchHint { role: Some(Role::Button), name: "Save".to_string(), exact: true };
        assert_eq!(hint("role:button name:\"Save\""), Some(save.clone()));
        assert_eq!(hint("name:Dialog >> role:button name:Save window:Chrome"), Some(save));
        let partial = SearchHint { role: None, name: "sav".to_string(), exact: false };
        assert_eq!(hint("name~:sav"), Some(partial.clone()));
        assert_eq!(hint("sav"), Some(partial));
        assert_eq!(hint("role:button id:7"), None);
        assert_eq!(hint("window:Chrome"), None);
    }
}
