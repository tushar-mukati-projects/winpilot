use crate::error::{Error, Result};

pub(crate) fn first_stray<'a>(given: &[(&'a str, bool)], allowed: impl Fn(&str) -> bool) -> Option<&'a str> {
    given.iter().find(|(name, given)| *given && !allowed(name)).map(|(name, _)| *name)
}

pub(crate) fn does_not_apply(field: &str, place: &str) -> Error {
    Error::invalid(format!("{field} does not apply to {place}"))
}

pub(crate) fn only_one(sources: &[(&str, bool)]) -> Result<()> {
    let given: Vec<&str> = sources.iter().filter(|(_, given)| *given).map(|(name, _)| *name).collect();
    if given.len() <= 1 {
        return Ok(());
    }
    let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
    Err(Error::invalid(format!("give only one of {}; got {}", either(&names), given.join(" and "))))
}

pub(crate) fn either(names: &[&str]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_given_field_outside_the_allowed_set_is_named() {
        let given = [("ms", false), ("target", true), ("text", true)];
        assert_eq!(first_stray(&given, |name| name == "target"), Some("text"));
        assert_eq!(first_stray(&given, |_| true), None);
        assert_eq!(does_not_apply("text", "wait until=idle").message, "text does not apply to wait until=idle");
    }

    #[test]
    fn exclusive_sources_name_both_offenders() {
        assert!(only_one(&[("target", true), ("region", false), ("monitor", false)]).is_ok());
        let error = only_one(&[("target", true), ("region", true), ("monitor", false)]).unwrap_err();
        assert_eq!(error.message, "give only one of target, region or monitor; got target and region");
    }

    #[test]
    fn lists_read_naturally() {
        assert_eq!(either(&[]), "");
        assert_eq!(either(&["record"]), "record");
        assert_eq!(either(&["elements", "apps"]), "elements or apps");
        assert_eq!(either(&["text", "screenshot", "record"]), "text, screenshot or record");
    }
}
