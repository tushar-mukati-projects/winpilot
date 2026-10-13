use std::{ffi::OsString, path::PathBuf};

pub fn dir_from_env(variable: &str) -> Option<PathBuf> {
    absolute(std::env::var_os(variable))
}

fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(value: &str) -> Option<PathBuf> {
        absolute(Some(OsString::from(value)))
    }

    #[test]
    fn missing_or_empty_values_are_missing() {
        assert_eq!(absolute(None), None);
        assert_eq!(check(""), None);
    }

    #[test]
    fn relative_values_are_missing() {
        assert_eq!(check("AppData\\Local"), None);
        assert_eq!(check("C:AppData"), None);
        assert_eq!(check("\\Users\\me\\AppData"), None);
        assert_eq!(check("."), None);
    }

    #[test]
    fn absolute_values_are_kept() {
        assert_eq!(check("C:\\Users\\me\\AppData\\Local"), Some(PathBuf::from("C:\\Users\\me\\AppData\\Local")));
        assert_eq!(check("\\\\server\\share\\profile"), Some(PathBuf::from("\\\\server\\share\\profile")));
    }
}
