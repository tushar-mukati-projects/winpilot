use deckhand_core::{
    Element, Error, Patterns, Result,
    keys::{Modifiers, parse_chord},
    protocol::{ClickHow, MouseButton},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClickOptions {
    pub(crate) button: MouseButton,
    pub(crate) count: u8,
    pub(crate) modifiers: Modifiers,
    pub(crate) how: ClickHow,
    pub(crate) glide_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClickPath {
    Pattern,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundPath {
    Pattern,
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternAction {
    Invoke,
    Toggle,
    Select,
    Expand,
    Collapse,
    DefaultAction,
}

impl Default for ClickOptions {
    fn default() -> Self {
        Self { button: MouseButton::Left, count: 1, modifiers: Modifiers::NONE, how: ClickHow::Auto, glide_ms: None }
    }
}

impl ClickOptions {
    pub(crate) fn new(button: MouseButton, count: u8, modifiers: Option<&str>, how: ClickHow) -> Result<Self> {
        let modifiers = modifiers.map(parse_modifiers).transpose()?.unwrap_or(Modifiers::NONE);
        Ok(Self { button, count, modifiers, how, glide_ms: None })
    }
}

impl PatternAction {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Invoke => "invoke",
            Self::Toggle => "toggle",
            Self::Select => "select",
            Self::Expand => "expand",
            Self::Collapse => "collapse",
            Self::DefaultAction => "default_action",
        }
    }
}

pub(crate) fn parse_modifiers(spec: &str) -> Result<Modifiers> {
    let chord = parse_chord(spec)?;
    if chord.key.is_some() {
        return Err(Error::invalid(format!(
            "modifiers must be modifier keys only, like ctrl or ctrl+shift; got '{spec}'"
        )));
    }
    Ok(chord.modifiers)
}

pub(crate) fn choose_path(options: ClickOptions, foreground: bool, patterns: Option<Patterns>) -> ClickPath {
    match options.how {
        ClickHow::Invoke => ClickPath::Pattern,
        ClickHow::Mouse => ClickPath::Mouse,
        ClickHow::Auto => auto_path(options.button, options.count, options.modifiers, foreground, patterns),
    }
}

pub(crate) fn background_path(options: ClickOptions, tracked: Option<&Element>) -> BackgroundPath {
    if options.how == ClickHow::Invoke {
        return BackgroundPath::Pattern;
    }
    let plain = options.button == MouseButton::Left && options.count == 1 && options.modifiers.is_empty();
    let operable = tracked.is_some_and(|element| !pattern_actions(element.patterns, element.state.expanded).is_empty());
    if plain && operable { BackgroundPath::Pattern } else { BackgroundPath::Message }
}

fn auto_path(
    button: MouseButton,
    count: u8,
    modifiers: Modifiers,
    foreground: bool,
    patterns: Option<Patterns>,
) -> ClickPath {
    let plain = button == MouseButton::Left && count == 1 && modifiers.is_empty();
    let operable =
        patterns.is_some_and(|found| found.invoke || found.toggle || found.selection_item || found.expand_collapse);
    if plain && !foreground && operable { ClickPath::Pattern } else { ClickPath::Mouse }
}

pub(crate) fn pattern_actions(patterns: Patterns, expanded: Option<bool>) -> Vec<PatternAction> {
    let flip = if expanded == Some(true) { PatternAction::Collapse } else { PatternAction::Expand };
    [
        (patterns.invoke, PatternAction::Invoke),
        (patterns.toggle, PatternAction::Toggle),
        (patterns.selection_item, PatternAction::Select),
        (patterns.expand_collapse, flip),
        (patterns.legacy, PatternAction::DefaultAction),
    ]
    .into_iter()
    .filter_map(|(available, action)| available.then_some(action))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invokable() -> Patterns {
        Patterns { invoke: true, ..Patterns::default() }
    }

    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, ..Modifiers::NONE }
    }

    #[test]
    fn plain_left_click_on_a_background_control_uses_the_pattern() {
        assert_eq!(auto_path(MouseButton::Left, 1, Modifiers::NONE, false, Some(invokable())), ClickPath::Pattern);
    }

    #[test]
    fn foreground_windows_get_a_real_click() {
        assert_eq!(auto_path(MouseButton::Left, 1, Modifiers::NONE, true, Some(invokable())), ClickPath::Mouse);
    }

    #[test]
    fn anything_but_a_plain_click_uses_the_mouse() {
        assert_eq!(auto_path(MouseButton::Right, 1, Modifiers::NONE, false, Some(invokable())), ClickPath::Mouse);
        assert_eq!(auto_path(MouseButton::Middle, 1, Modifiers::NONE, false, Some(invokable())), ClickPath::Mouse);
        assert_eq!(auto_path(MouseButton::Left, 2, Modifiers::NONE, false, Some(invokable())), ClickPath::Mouse);
        assert_eq!(auto_path(MouseButton::Left, 1, ctrl(), false, Some(invokable())), ClickPath::Mouse);
    }

    #[test]
    fn points_and_pattern_less_elements_use_the_mouse() {
        assert_eq!(auto_path(MouseButton::Left, 1, Modifiers::NONE, false, None), ClickPath::Mouse);
        let inert = Some(Patterns { value: true, scroll: true, legacy: true, ..Patterns::default() });
        assert_eq!(auto_path(MouseButton::Left, 1, Modifiers::NONE, false, inert), ClickPath::Mouse);
    }

    #[test]
    fn each_operable_pattern_qualifies_for_auto() {
        let each = [
            Patterns { toggle: true, ..Patterns::default() },
            Patterns { selection_item: true, ..Patterns::default() },
            Patterns { expand_collapse: true, ..Patterns::default() },
        ];
        for patterns in each {
            assert_eq!(auto_path(MouseButton::Left, 1, Modifiers::NONE, false, Some(patterns)), ClickPath::Pattern);
        }
    }

    #[test]
    fn explicit_how_overrides_auto() {
        let invoke = ClickOptions { how: ClickHow::Invoke, ..ClickOptions::default() };
        assert_eq!(choose_path(invoke, true, None), ClickPath::Pattern);
        let mouse = ClickOptions { how: ClickHow::Mouse, ..ClickOptions::default() };
        assert_eq!(choose_path(mouse, false, Some(invokable())), ClickPath::Mouse);
        assert_eq!(choose_path(ClickOptions::default(), false, Some(invokable())), ClickPath::Pattern);
    }

    #[test]
    fn background_clicks_use_a_pattern_only_for_plain_clicks_on_operable_elements() {
        use deckhand_core::Role;

        use crate::test_fixture::element;

        let mut button = element(1, 1, Role::Button, "OK");
        button.patterns.invoke = true;
        let canvas = element(2, 1, Role::Pane, "Timeline");
        let plain = ClickOptions::default();
        assert_eq!(background_path(plain, Some(&button)), BackgroundPath::Pattern);
        assert_eq!(background_path(plain, Some(&canvas)), BackgroundPath::Message);
        assert_eq!(background_path(plain, None), BackgroundPath::Message);
        let right = ClickOptions { button: MouseButton::Right, ..plain };
        assert_eq!(background_path(right, Some(&button)), BackgroundPath::Message);
        let double = ClickOptions { count: 2, ..plain };
        assert_eq!(background_path(double, Some(&button)), BackgroundPath::Message);
        let invoke = ClickOptions { how: ClickHow::Invoke, ..plain };
        assert_eq!(background_path(invoke, None), BackgroundPath::Pattern);
    }

    #[test]
    fn pattern_actions_follow_the_preference_order() {
        let everything = Patterns {
            invoke: true,
            toggle: true,
            selection_item: true,
            expand_collapse: true,
            legacy: true,
            ..Patterns::default()
        };
        assert_eq!(
            pattern_actions(everything, None),
            [
                PatternAction::Invoke,
                PatternAction::Toggle,
                PatternAction::Select,
                PatternAction::Expand,
                PatternAction::DefaultAction
            ]
        );
        assert!(pattern_actions(Patterns::default(), None).is_empty());
    }

    #[test]
    fn an_expanded_element_collapses() {
        let expander = Patterns { expand_collapse: true, ..Patterns::default() };
        assert_eq!(pattern_actions(expander, Some(true)), [PatternAction::Collapse]);
        assert_eq!(pattern_actions(expander, Some(false)), [PatternAction::Expand]);
    }

    #[test]
    fn options_validate_count_and_modifiers() {
        let options = ClickOptions::new(MouseButton::Left, 2, Some("ctrl+shift"), ClickHow::Auto).unwrap();
        assert_eq!(options.modifiers, Modifiers { ctrl: true, shift: true, ..Modifiers::NONE });
        assert_eq!(options.count, 2);
        let error = ClickOptions::new(MouseButton::Left, 1, Some("ctrl+a"), ClickHow::Auto).unwrap_err();
        assert!(error.message.contains("modifier keys only"), "{}", error.message);
    }

    #[test]
    fn default_options_are_a_plain_auto_click() {
        let options = ClickOptions::default();
        assert_eq!((options.button, options.count, options.how), (MouseButton::Left, 1, ClickHow::Auto));
        assert!(options.modifiers.is_empty());
    }
}
