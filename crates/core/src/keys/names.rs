use super::{Modifier, NamedKey};

pub(super) fn modifier_named(name: &str) -> Option<Modifier> {
    match name {
        "ctrl" | "control" | "ctl" => Some(Modifier::Ctrl),
        "alt" | "option" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        "win" | "windows" | "meta" | "super" => Some(Modifier::Win),
        _ => None,
    }
}

pub(super) fn named_key(name: &str) -> Option<NamedKey> {
    Some(match name {
        "enter" | "return" => NamedKey::Enter,
        "esc" | "escape" => NamedKey::Escape,
        "tab" => NamedKey::Tab,
        "space" => NamedKey::Space,
        "backspace" | "bksp" | "back" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "insert" | "ins" => NamedKey::Insert,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "pageup" | "pgup" => NamedKey::PageUp,
        "pagedown" | "pgdn" => NamedKey::PageDown,
        "up" | "arrowup" => NamedKey::Up,
        "down" | "arrowdown" => NamedKey::Down,
        "left" | "arrowleft" => NamedKey::Left,
        "right" | "arrowright" => NamedKey::Right,
        "printscreen" | "prtsc" | "prtscr" => NamedKey::PrintScreen,
        "pause" | "break" => NamedKey::Pause,
        "capslock" => NamedKey::CapsLock,
        "numlock" => NamedKey::NumLock,
        "scrolllock" => NamedKey::ScrollLock,
        "contextmenu" | "menu" | "apps" | "application" => NamedKey::ContextMenu,
        "volumeup" => NamedKey::VolumeUp,
        "volumedown" => NamedKey::VolumeDown,
        "volumemute" | "mute" => NamedKey::VolumeMute,
        "playpause" | "mediaplaypause" => NamedKey::MediaPlayPause,
        "nexttrack" | "medianext" => NamedKey::MediaNext,
        "prevtrack" | "mediaprevious" => NamedKey::MediaPrevious,
        "browserback" => NamedKey::BrowserBack,
        "browserforward" => NamedKey::BrowserForward,
        _ => return None,
    })
}

pub(super) fn function_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix('f')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(digits.parse().unwrap_or(u32::MAX))
}

pub(super) fn char_alias(name: &str) -> Option<char> {
    match name {
        "plus" => Some('+'),
        "minus" => Some('-'),
        "comma" => Some(','),
        _ => None,
    }
}

pub(super) fn single_char(part: &str) -> Option<char> {
    let mut chars = part.chars();
    let ch = chars.next()?;
    chars.next().is_none().then(|| lowercase_char(ch))
}

fn lowercase_char(ch: char) -> char {
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => ch,
    }
}
