use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Aurora,
    Sunset,
    Ocean,
    Mono,
}

impl Theme {
    pub fn colors(self) -> Vec<HexColor> {
        let colors: &[[u8; 3]] = match self {
            Self::Aurora => {
                &[[0x7C, 0x3A, 0xED], [0x4F, 0x46, 0xE5], [0x0E, 0xA5, 0xE9], [0x22, 0xD3, 0xEE], [0xF4, 0x72, 0xB6]]
            }
            Self::Sunset => &[[0xF9, 0x73, 0x16], [0xEF, 0x44, 0x44], [0xEC, 0x48, 0x99], [0xA8, 0x55, 0xF7]],
            Self::Ocean => &[[0x0E, 0xA5, 0xE9], [0x06, 0xB6, 0xD4], [0x14, 0xB8, 0xA6], [0x3B, 0x82, 0xF6]],
            Self::Mono => &[[0xFF, 0xFF, 0xFF], [0xD4, 0xD4, 0xD8], [0xA1, 0xA1, 0xAA], [0xE4, 0xE4, 0xE7]],
        };
        colors.iter().copied().map(HexColor).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor(pub [u8; 3]);

impl HexColor {
    const fn from_u32(value: u32) -> Self {
        Self([(value >> 16) as u8, (value >> 8) as u8, value as u8])
    }
}

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let digits = text.trim().trim_start_matches('#');
        let valid = digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit());
        let parsed = valid.then(|| u32::from_str_radix(digits, 16).ok()).flatten();
        parsed.map(Self::from_u32).ok_or_else(|| format!("overlay colour \"{text}\" is not a hex colour like #7C3AED"))
    }
}

impl From<HexColor> for String {
    fn from(color: HexColor) -> Self {
        color.to_string()
    }
}

impl fmt::Display for HexColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [red, green, blue] = self.0;
        write!(formatter, "#{red:02X}{green:02X}{blue:02X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_parse_with_or_without_a_hash() {
        assert_eq!(HexColor::try_from("#7C3AED".to_string()), Ok(HexColor([0x7C, 0x3A, 0xED])));
        assert_eq!(HexColor::try_from("22d3ee".to_string()), Ok(HexColor([0x22, 0xD3, 0xEE])));
        assert_eq!(HexColor([1, 2, 255]).to_string(), "#0102FF");
    }

    #[test]
    fn bad_hex_colours_are_named_in_the_error() {
        for bad in ["#12345", "#GGGGGG", "", "red", "#1234567", "+12345"] {
            let error = HexColor::try_from(bad.to_string()).unwrap_err();
            assert!(error.contains(&format!("\"{bad}\"")), "{error}");
        }
    }

    #[test]
    fn every_theme_loops_through_at_least_four_colours() {
        for theme in [Theme::Aurora, Theme::Sunset, Theme::Ocean, Theme::Mono] {
            assert!(theme.colors().len() >= 4, "{theme:?}");
        }
        assert_eq!(Theme::Aurora.colors()[0], HexColor([0x7C, 0x3A, 0xED]));
        assert_eq!(Theme::default(), Theme::Aurora);
    }

    #[test]
    fn themes_read_as_snake_case_names() {
        assert_eq!(serde_json::from_str::<Theme>("\"sunset\"").unwrap(), Theme::Sunset);
        assert!(serde_json::from_str::<Theme>("\"neon\"").is_err());
    }
}
