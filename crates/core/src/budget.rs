use std::time::Duration;

use crate::{
    glide_timing::MAX_GLIDE_MS,
    keys::parse_keys,
    observe::{ObserveArgs, ObserveMode},
    protocol::Call,
    recording::RecordAction,
    steps::{MouseAction, Step, WindowAction},
    wait::{MAX_PLAIN_WAIT, MAX_WAIT, Until, WaitArgs},
};

const STEP_BUDGET: Duration = Duration::from_secs(6);
const ACT_OVERHEAD: Duration = Duration::from_secs(10);
const MAX_DEADLINE: Duration = Duration::from_secs(180);
const LAUNCH_BUDGET: Duration = Duration::from_secs(22);
const DEFAULT_PICK: Duration = Duration::from_secs(15);
const PICK_OVERHEAD: Duration = Duration::from_secs(10);
const WAIT_SLACK: Duration = Duration::from_secs(1);
const CHARS_PER_EXTRA_SECOND: u64 = 2000;
const KEY_PRESS_BUDGET: Duration = Duration::from_millis(100);

impl Call {
    pub fn deadline_with_glide(&self, default_glide_ms: u64) -> Duration {
        let deadline = match self {
            Self::Observe(args) => observe_budget(args),
            Self::Act(args) => {
                let steps = args.steps.iter().map(|step| step_budget(step, default_glide_ms));
                steps.fold(ACT_OVERHEAD, Duration::saturating_add)
            }
        };
        deadline.min(MAX_DEADLINE)
    }
}

const fn observe_budget(args: &ObserveArgs) -> Duration {
    match args.mode {
        ObserveMode::Elements | ObserveMode::Apps => Duration::from_secs(10),
        ObserveMode::Text => Duration::from_secs(20),
        ObserveMode::Screenshot => Duration::from_secs(8),
        ObserveMode::Record => record_budget(args.action),
        ObserveMode::Windows | ObserveMode::Monitors | ObserveMode::Clipboard => Duration::from_secs(5),
        ObserveMode::Status => Duration::from_secs(3),
    }
}

const fn record_budget(action: Option<RecordAction>) -> Duration {
    match action {
        Some(RecordAction::Start) => Duration::from_secs(15),
        Some(RecordAction::Stop) => Duration::from_secs(60),
        Some(RecordAction::Status) | None => Duration::from_secs(3),
    }
}

fn step_budget(step: &Step, default_glide_ms: u64) -> Duration {
    match step {
        Step::Wait(args) => wait_budget(args),
        Step::Type { text, .. } | Step::Set { value: Some(text), .. } => STEP_BUDGET.saturating_add(typing_time(text)),
        Step::Key { keys, repeat, .. } => STEP_BUDGET.saturating_add(key_time(keys, *repeat)),
        Step::Mouse { action: MouseAction::Drag, glide_ms, .. } => {
            STEP_BUDGET.saturating_add(glide_time(glide_ms.unwrap_or(default_glide_ms), 2))
        }
        Step::Click { glide_ms, .. } | Step::Mouse { glide_ms, .. } => {
            STEP_BUDGET.saturating_add(glide_time(glide_ms.unwrap_or(default_glide_ms), 1))
        }
        Step::Window { action: WindowAction::Launch, .. } => LAUNCH_BUDGET,
        Step::Pick { timeout_ms, .. } => pick_wait(*timeout_ms).saturating_add(PICK_OVERHEAD),
        Step::Set { .. } | Step::Focus { .. } | Step::Window { .. } | Step::Clipboard { .. } | Step::Notify { .. } => {
            STEP_BUDGET
        }
    }
}

fn wait_budget(args: &WaitArgs) -> Duration {
    let waited = match args.until() {
        Until::Time => Duration::from_millis(args.ms.unwrap_or(0)).min(MAX_PLAIN_WAIT),
        _ => args.timeout(),
    };
    waited.saturating_add(WAIT_SLACK)
}

pub fn pick_wait(timeout_ms: Option<u64>) -> Duration {
    timeout_ms.map_or(DEFAULT_PICK, Duration::from_millis).min(MAX_WAIT)
}

fn glide_time(glide_ms: u64, moves: u32) -> Duration {
    Duration::from_millis(glide_ms.min(MAX_GLIDE_MS)).saturating_mul(moves)
}

fn typing_time(text: &str) -> Duration {
    let chars = text.chars().count() as u64;
    Duration::from_secs(chars.div_ceil(CHARS_PER_EXTRA_SECOND))
}

fn key_time(keys: &str, repeat: u32) -> Duration {
    let chords = parse_keys(keys).map_or(1, |chords| chords.len());
    let presses = u32::try_from(chords).unwrap_or(u32::MAX).saturating_mul(repeat);
    KEY_PRESS_BUDGET.saturating_mul(presses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steps::{ActArgs, AfterMode, TypeHow};

    fn act(steps: Vec<Step>) -> Call {
        Call::Act(ActArgs { steps, after: AfterMode::Diff, stop_on_error: true, background: None })
    }

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    fn observe(json: &str) -> Duration {
        Call::Observe(serde_json::from_str(json).unwrap()).deadline_with_glide(0)
    }

    #[test]
    fn observe_deadlines_follow_the_mode() {
        let cases = [
            ("{}", 10),
            (r#"{"mode":"text"}"#, 20),
            (r#"{"mode":"screenshot"}"#, 8),
            (r#"{"mode":"record","action":"start"}"#, 15),
            (r#"{"mode":"record","action":"stop"}"#, 60),
            (r#"{"mode":"record","action":"status"}"#, 3),
            (r#"{"mode":"windows"}"#, 5),
            (r#"{"mode":"monitors"}"#, 5),
            (r#"{"mode":"apps"}"#, 10),
            (r#"{"mode":"clipboard"}"#, 5),
            (r#"{"mode":"status"}"#, 3),
        ];
        for (json, seconds) in cases {
            assert_eq!(observe(json), Duration::from_secs(seconds), "{json}");
        }
    }

    #[test]
    fn act_deadline_grows_with_steps_and_is_capped() {
        let pause = step(r#"{"do":"wait","ms":1000}"#);
        assert_eq!(act(vec![pause.clone()]).deadline_with_glide(0), ACT_OVERHEAD + Duration::from_secs(2));
        assert_eq!(act(vec![pause; 100]).deadline_with_glide(0), MAX_DEADLINE);
        assert_eq!(
            act(vec![step(r#"{"do":"wait","ms":60000}"#)]).deadline_with_glide(0),
            ACT_OVERHEAD + Duration::from_secs(6)
        );
        let watched = step(r#"{"do":"wait","until":"appears","target":"e1","timeout_ms":8000}"#);
        assert_eq!(act(vec![watched]).deadline_with_glide(0), ACT_OVERHEAD + Duration::from_secs(9));
    }

    #[test]
    fn act_deadline_includes_explicit_glides() {
        let moved = step(r#"{"do":"mouse","action":"move","target":"e1","glide_ms":2000}"#);
        assert_eq!(act(vec![moved]).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(2));
        let dragged = step(r#"{"do":"mouse","action":"drag","target":"e1","to":"e2","glide_ms":1500}"#);
        assert_eq!(act(vec![dragged]).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(3));
        assert_eq!(
            act(vec![step(r#"{"do":"mouse","action":"up"}"#)]).deadline_with_glide(0),
            ACT_OVERHEAD + STEP_BUDGET
        );
    }

    #[test]
    fn act_deadline_budgets_the_configured_glide_when_a_step_names_none() {
        let clicked = step(r#"{"do":"click","target":"e1"}"#);
        assert_eq!(
            act(vec![clicked.clone()]).deadline_with_glide(500),
            ACT_OVERHEAD + STEP_BUDGET + Duration::from_millis(500)
        );
        let dragged = step(r#"{"do":"mouse","action":"drag","target":"e1","to":"e2"}"#);
        assert_eq!(act(vec![dragged]).deadline_with_glide(1000), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(2));
        let explicit = step(r#"{"do":"click","target":"e1","glide_ms":0}"#);
        assert_eq!(act(vec![explicit]).deadline_with_glide(3000), ACT_OVERHEAD + STEP_BUDGET);
        assert_eq!(act(vec![clicked]).deadline_with_glide(60_000), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(3));
    }

    #[test]
    fn act_deadline_accounts_for_typing_and_key_repeats() {
        let typed = |text: String| act(vec![Step::Type { target: None, text, enter: false, how: TypeHow::default() }]);
        assert_eq!(typed("hi".to_string()).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(1));
        assert_eq!(
            typed("a".repeat(10_000)).deadline_with_glide(0),
            ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(5)
        );
        assert_eq!(typed(String::new()).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET);
        let set = step(r#"{"do":"set","target":"e1","value":"abc"}"#);
        assert_eq!(act(vec![set]).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(1));
        let keys = step(r#"{"do":"key","keys":"ctrl+k ctrl+s","repeat":50}"#);
        assert_eq!(act(vec![keys]).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(10));
        let unparsable = Step::Key {
            keys: "bogus".to_string(),
            target: None,
            repeat: u32::MAX,
            action: crate::steps::KeyAction::Press,
        };
        assert_eq!(act(vec![unparsable]).deadline_with_glide(0), MAX_DEADLINE);
    }

    #[test]
    fn launch_and_pick_get_room_to_wait() {
        let launch = step(r#"{"do":"window","action":"launch","app":"Notepad"}"#);
        assert_eq!(act(vec![launch]).deadline_with_glide(0), ACT_OVERHEAD + LAUNCH_BUDGET);
        let pick = step(r#"{"do":"pick","paths":["C:\\a.txt"]}"#);
        assert_eq!(act(vec![pick]).deadline_with_glide(0), ACT_OVERHEAD + DEFAULT_PICK + PICK_OVERHEAD);
        let slow = step(r#"{"do":"pick","paths":["C:\\a.txt"],"timeout_ms":999999}"#);
        assert_eq!(act(vec![slow]).deadline_with_glide(0), ACT_OVERHEAD + MAX_WAIT + PICK_OVERHEAD);
        let close = step(r#"{"do":"window","action":"close","target":"w1"}"#);
        assert_eq!(act(vec![close]).deadline_with_glide(0), ACT_OVERHEAD + STEP_BUDGET);
    }
}
