use std::ffi::{OsStr, OsString};

pub const DEFAULT_TITLE: &str = "Deckhand Fixture";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub title: OsString,
    pub position: Option<Position>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Self {
        let mut options = Self { title: OsString::from(DEFAULT_TITLE), position: None };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--title" {
                if let Some(title) = args.next() {
                    options.title = title;
                }
            } else if arg == "--pos" {
                options.position = args.next().as_deref().and_then(parse_position).or(options.position);
            }
        }
        options
    }
}

fn parse_position(text: &OsStr) -> Option<Position> {
    let (x, y) = text.to_str()?.split_once(',')?;
    Some(Position { x: x.trim().parse().ok()?, y: y.trim().parse().ok()? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        Options::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn defaults_without_arguments() {
        let options = parse(&[]);
        assert_eq!(options.title, DEFAULT_TITLE);
        assert_eq!(options.position, None);
    }

    #[test]
    fn title_overrides_default() {
        assert_eq!(parse(&["--title", "Fixture B"]).title, "Fixture B");
    }

    #[test]
    fn position_parses_coordinates() {
        assert_eq!(parse(&["--pos", "100,200"]).position, Some(Position { x: 100, y: 200 }));
    }

    #[test]
    fn position_accepts_negative_and_spaced_coordinates() {
        assert_eq!(parse(&["--pos", "-1920, -40"]).position, Some(Position { x: -1920, y: -40 }));
    }

    #[test]
    fn malformed_positions_are_ignored() {
        for bad in ["", "10", "10,", ",10", "a,b", "1,2,3", "99999999999,1"] {
            assert_eq!(parse(&["--pos", bad]).position, None, "{bad}");
        }
    }

    #[test]
    fn malformed_position_keeps_earlier_valid_one() {
        assert_eq!(parse(&["--pos", "5,6", "--pos", "x"]).position, Some(Position { x: 5, y: 6 }));
    }

    #[test]
    fn unknown_and_help_arguments_are_ignored() {
        let options = parse(&["--help", "-v", "stray", "--title", "T", "--verbose"]);
        assert_eq!(options.title, "T");
        assert_eq!(options.position, None);
    }

    #[test]
    fn missing_values_keep_defaults() {
        assert_eq!(parse(&["--title"]).title, DEFAULT_TITLE);
        assert_eq!(parse(&["--pos"]).position, None);
    }

    #[test]
    fn later_arguments_win() {
        let options = parse(&["--title", "A", "--pos", "1,2", "--title", "B", "--pos", "3,4"]);
        assert_eq!(options.title, "B");
        assert_eq!(options.position, Some(Position { x: 3, y: 4 }));
    }
}
