const KNOWN_AGENTS: [(&str, &str); 8] = [
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("cursor", "Cursor"),
    ("copilot", "Copilot"),
    ("vscode", "Copilot"),
    ("visual studio code", "Copilot"),
    ("windsurf", "Windsurf"),
    ("gemini", "Gemini"),
];
const FALLBACK: &str = "AI agent";
const MAX_LABEL_CHARS: usize = 24;
const ELLIPSIS: char = '\u{2026}';
const INVISIBLE_FORMATTING: [(char, char); 4] =
    [('\u{200B}', '\u{200F}'), ('\u{202A}', '\u{202E}'), ('\u{2066}', '\u{2069}'), ('\u{FEFF}', '\u{FEFF}')];

pub(crate) fn agent_label(client_name: &str) -> String {
    let cleaned = sanitized_label(client_name);
    let folded = cleaned.to_lowercase();
    if let Some((_, label)) = KNOWN_AGENTS.iter().find(|(needle, _)| folded.contains(needle)) {
        return (*label).to_string();
    }
    let words: Vec<String> = cleaned
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .map(title_cased)
        .collect();
    let label = sanitized_label(&words.join(" "));
    if label.is_empty() { FALLBACK.to_string() } else { label }
}

pub(crate) fn sanitized_label(text: &str) -> String {
    let visible: String = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !is_invisible_formatting(*c))
        .collect();
    capped(&visible.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn is_invisible_formatting(c: char) -> bool {
    INVISIBLE_FORMATTING.iter().any(|(first, last)| (*first..=*last).contains(&c))
}

fn capped(label: &str) -> String {
    if label.chars().count() <= MAX_LABEL_CHARS {
        return label.to_string();
    }
    let kept: String = label.chars().take(MAX_LABEL_CHARS - 1).collect();
    format!("{}{ELLIPSIS}", kept.trim_end())
}

fn title_cased(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_clients_get_their_product_names() {
        assert_eq!(agent_label("claude-code"), "Claude");
        assert_eq!(agent_label("Claude Desktop"), "Claude");
        assert_eq!(agent_label("codex-mcp-client"), "Codex");
        assert_eq!(agent_label("cursor-vscode"), "Cursor");
        assert_eq!(agent_label("GitHub Copilot"), "Copilot");
        assert_eq!(agent_label("Visual Studio Code"), "Copilot");
        assert_eq!(agent_label("vscode"), "Copilot");
        assert_eq!(agent_label("windsurf-client"), "Windsurf");
        assert_eq!(agent_label("gemini-cli"), "Gemini");
        assert_eq!(agent_label("cl\u{200B}aude-code"), "Claude");
    }

    #[test]
    fn other_clients_are_title_cased_and_capped() {
        assert_eq!(agent_label("my_custom-agent"), "My Custom Agent");
        assert_eq!(agent_label("goose"), "Goose");
        let long = agent_label("a very long agent name that keeps going");
        assert_eq!(long, format!("A Very Long Agent Name{ELLIPSIS}"));
        assert!(long.ends_with(ELLIPSIS), "{long}");
        assert_eq!(agent_label("ünïcode agent"), "Ünïcode Agent");
    }

    #[test]
    fn empty_names_fall_back_to_a_neutral_label() {
        assert_eq!(agent_label(""), "AI agent");
        assert_eq!(agent_label("  -_ "), "AI agent");
        assert_eq!(agent_label("\u{202E}\u{200B}\u{7}"), "AI agent");
    }

    #[test]
    fn labels_lose_control_and_bidi_characters() {
        assert_eq!(sanitized_label("Hel\u{202E}per\u{2066}\u{2069}"), "Helper");
        assert_eq!(sanitized_label("\u{FEFF}Bot\u{200F}\u{200D}"), "Bot");
        assert_eq!(sanitized_label("line\none\ttwo\u{1B}[31m"), "line one two[31m");
        assert_eq!(sanitized_label("  many    spaces\u{3000}here  "), "many spaces here");
    }

    #[test]
    fn labels_are_capped_on_a_character_boundary_with_an_ellipsis() {
        assert_eq!(sanitized_label(&"x".repeat(24)), "x".repeat(24));
        let capped = sanitized_label(&"é".repeat(40));
        assert_eq!(capped.chars().count(), MAX_LABEL_CHARS);
        assert_eq!(capped, format!("{}{ELLIPSIS}", "é".repeat(23)));
        assert_eq!(sanitized_label("twenty two characters_ end"), format!("twenty two characters_{ELLIPSIS}"));
        assert_eq!(sanitized_label(""), "");
    }
}
