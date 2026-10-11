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

pub fn agent_label(client_name: &str) -> String {
    let folded = client_name.to_lowercase();
    if let Some((_, label)) = KNOWN_AGENTS.iter().find(|(needle, _)| folded.contains(needle)) {
        return (*label).to_string();
    }
    let words: Vec<String> = client_name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .map(title_cased)
        .collect();
    if words.is_empty() {
        return FALLBACK.to_string();
    }
    words.join(" ").chars().take(MAX_LABEL_CHARS).collect::<String>().trim_end().to_string()
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
    }

    #[test]
    fn other_clients_are_title_cased_and_capped() {
        assert_eq!(agent_label("my_custom-agent"), "My Custom Agent");
        assert_eq!(agent_label("goose"), "Goose");
        assert_eq!(agent_label("a very long agent name that keeps going").chars().count(), MAX_LABEL_CHARS);
        assert_eq!(agent_label("ünïcode agent"), "Ünïcode Agent");
    }

    #[test]
    fn empty_names_fall_back_to_a_neutral_label() {
        assert_eq!(agent_label(""), "AI agent");
        assert_eq!(agent_label("  -_ "), "AI agent");
    }
}
