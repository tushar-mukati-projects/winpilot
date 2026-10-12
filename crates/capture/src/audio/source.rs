#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioSource {
    System,
    Mic,
    Both,
}

impl AudioSource {
    pub const fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Mic => "mic",
            Self::Both => "both",
        }
    }

    pub(crate) const fn includes_system(self) -> bool {
        matches!(self, Self::System | Self::Both)
    }

    pub(crate) const fn includes_mic(self) -> bool {
        matches!(self, Self::Mic | Self::Both)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_name_the_endpoints_they_open() {
        assert_eq!(
            [AudioSource::System, AudioSource::Mic, AudioSource::Both].map(AudioSource::name),
            ["system", "mic", "both"]
        );
        assert!(AudioSource::System.includes_system() && !AudioSource::System.includes_mic());
        assert!(!AudioSource::Mic.includes_system() && AudioSource::Mic.includes_mic());
        assert!(AudioSource::Both.includes_system() && AudioSource::Both.includes_mic());
    }
}
