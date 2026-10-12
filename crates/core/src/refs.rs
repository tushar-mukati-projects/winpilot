use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ElementRef(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WindowRef(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImageRef(pub u32);

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "e{}", self.0)
    }
}

impl fmt::Display for WindowRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "w{}", self.0)
    }
}

impl fmt::Display for ImageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "i{}", self.0)
    }
}

fn strip_prefix<'a>(text: &'a str, prefix: char, kind: &str) -> Result<&'a str, Error> {
    text.strip_prefix(prefix)
        .filter(|rest| !rest.is_empty())
        .ok_or_else(|| Error::invalid(format!("'{text}' is not {kind} ref (expected {prefix}…)")))
}

impl FromStr for ElementRef {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let digits = strip_prefix(text, 'e', "an element")?;
        digits.parse().map(Self).map_err(|_| Error::invalid(format!("'{text}' is not an element ref")))
    }
}

impl FromStr for WindowRef {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let digits = strip_prefix(text, 'w', "a window")?;
        digits.parse().map(Self).map_err(|_| Error::invalid(format!("'{text}' is not a window ref")))
    }
}

impl FromStr for ImageRef {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let digits = strip_prefix(text, 'i', "an image")?;
        digits.parse().map(Self).map_err(|_| Error::invalid(format!("'{text}' is not an image ref")))
    }
}

pub fn looks_like_element_ref(text: &str) -> bool {
    looks_like_ref(text, 'e')
}

pub fn looks_like_window_ref(text: &str) -> bool {
    looks_like_ref(text, 'w')
}

fn looks_like_ref(text: &str, prefix: char) -> bool {
    text.strip_prefix(prefix).is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_ref_round_trips() {
        let parsed: ElementRef = "e42".parse().unwrap();
        assert_eq!(parsed, ElementRef(42));
        assert_eq!(parsed.to_string(), "e42");
    }

    #[test]
    fn window_ref_is_decimal() {
        let parsed: WindowRef = "w2032546".parse().unwrap();
        assert_eq!(parsed, WindowRef(2_032_546));
        assert_eq!(parsed.to_string(), "w2032546");
    }

    #[test]
    fn rejects_wrong_prefix_and_empty() {
        assert!("x12".parse::<ElementRef>().is_err());
        assert!("e".parse::<ElementRef>().is_err());
        assert!("w".parse::<WindowRef>().is_err());
        assert!("wzz".parse::<WindowRef>().is_err());
    }

    #[test]
    fn ref_shapes() {
        assert!(looks_like_element_ref("e7"));
        assert!(!looks_like_element_ref("edit"));
        assert!(looks_like_window_ref("w12345"));
        assert!(!looks_like_window_ref("web"));
        assert!(!looks_like_window_ref("window"));
    }
}
