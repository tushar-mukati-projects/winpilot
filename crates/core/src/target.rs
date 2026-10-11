use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    geometry::{COORD_LIMIT, Point},
    refs::{ElementRef, ImageRef, WindowRef, looks_like_element_ref, looks_like_window_ref},
    selector::Selector,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, expecting = "a ref or selector string, or {\"x\":int,\"y\":int,\"img\":\"i3\"}")]
#[schemars(
    description = "What to act on: an element ref like e12, a window ref like w2032546, a selector like role:button name:\"Save\" or window:\"Notepad\" >> name:\"File\", or a point {\"x\":640,\"y\":360,\"img\":\"i3\"} in screenshot pixels (omit img for screen pixels)"
)]
pub enum TargetSpec {
    Text(String),
    Point(PointSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointSpec {
    pub x: i32,
    pub y: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Screenshot ref like i3 whose pixels x and y are measured in; omit for physical screen pixels."
    )]
    pub img: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Element(ElementRef),
    Window(WindowRef),
    Selector(Selector),
    Point { point: Point, image: Option<ImageRef> },
}

impl TargetSpec {
    pub fn parse(&self) -> Result<Target> {
        match self {
            Self::Text(text) => parse_text_target(text),
            Self::Point(spec) => spec.parse(),
        }
    }
}

impl PointSpec {
    fn parse(&self) -> Result<Target> {
        let point = Point::new(self.x, self.y);
        if !point.is_plausible() {
            return Err(Error::invalid(format!(
                "point {},{} is outside the plausible range of ±{COORD_LIMIT} pixels",
                self.x, self.y
            )));
        }
        let image = self.img.as_deref().map(str::parse).transpose()?;
        Ok(Target::Point { point, image })
    }
}

pub fn parse_text_target(text: &str) -> Result<Target> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::invalid("empty target"));
    }
    if looks_like_element_ref(text) {
        return text.parse().map(Target::Element);
    }
    if looks_like_window_ref(text) {
        return text.parse().map(Target::Window);
    }
    text.parse().map(Target::Selector)
}

impl fmt::Display for TargetSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.write_str(text),
            Self::Point(PointSpec { x, y, img: Some(image) }) => write!(f, "{x},{y}@{image}"),
            Self::Point(PointSpec { x, y, img: None }) => write!(f, "{x},{y}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn refs_are_recognised_before_selectors() {
        assert_eq!(parse_text_target("e12").unwrap(), Target::Element(ElementRef(12)));
        assert_eq!(parse_text_target(" w4242 ").unwrap(), Target::Window(WindowRef(4242)));
    }

    #[test]
    fn point_with_image() {
        let spec = TargetSpec::Point(PointSpec { x: 5, y: 6, img: Some("i3".to_string()) });
        assert_eq!(spec.parse().unwrap(), Target::Point { point: Point::new(5, 6), image: Some(ImageRef(3)) });
        assert_eq!(spec.to_string(), "5,6@i3");
    }

    #[test]
    fn deserializes_string_or_point() {
        let text: TargetSpec = serde_json::from_str("\"e1\"").unwrap();
        assert_eq!(text, TargetSpec::Text("e1".to_string()));
        let point: TargetSpec = serde_json::from_str("{\"x\":1,\"y\":2}").unwrap();
        assert_eq!(point, TargetSpec::Point(PointSpec { x: 1, y: 2, img: None }));
    }

    #[test]
    fn implausible_points_are_rejected() {
        let far = TargetSpec::Point(PointSpec { x: i32::MAX, y: 0, img: None });
        let error = far.parse().unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(error.message, format!("point {},0 is outside the plausible range of ±100000 pixels", i32::MAX));
        let edge = TargetSpec::Point(PointSpec { x: -COORD_LIMIT, y: COORD_LIMIT, img: None });
        assert!(edge.parse().is_ok());
    }

    #[test]
    fn misspelled_point_fields_are_rejected() {
        for json in [r#"{"x":640,"y":360,"image":"i3"}"#, r#"{"x":1,"y":2,"z":3}"#] {
            assert!(serde_json::from_str::<PointSpec>(json).is_err(), "{json}");
            assert!(serde_json::from_str::<TargetSpec>(json).is_err(), "{json}");
        }
    }

    #[test]
    fn bad_targets_explain_the_expected_shapes() {
        let expected = r#"a ref or selector string, or {"x":int,"y":int,"img":"i3"}"#;
        for json in [r#"{"x":640,"y":360,"image":"i3"}"#, r#"{"x":640.5,"y":360}"#, "42", "true", "null"] {
            let error = serde_json::from_str::<TargetSpec>(json).unwrap_err().to_string();
            assert_eq!(error, expected, "{json}");
        }
    }

    #[test]
    fn empty_target_is_invalid() {
        assert!(parse_text_target("   ").is_err());
    }
}
