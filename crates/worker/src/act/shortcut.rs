use std::path::Path;

use deckhand_core::{Error, Result};

const SHORTCUT_EXTENSIONS: [&str; 3] = ["lnk", "url", "library-ms"];

pub(crate) fn refuse_shortcut(path: &Path, instead: &str) -> Result<()> {
    if is_shortcut(path) {
        return Err(Error::invalid(format!("{} is a shortcut; {instead}", path.display())));
    }
    Ok(())
}

fn is_shortcut(path: &Path) -> bool {
    let extension = path.extension().map(|extension| extension.to_string_lossy().to_lowercase());
    extension.is_some_and(|extension| SHORTCUT_EXTENSIONS.contains(&extension.as_str()))
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    #[test]
    fn shortcuts_are_refused_with_the_way_out() {
        for path in ["C:\\Data\\Report.lnk", "C:\\Data\\site.URL", "C:\\Data\\Documents.library-ms"] {
            let error = refuse_shortcut(Path::new(path), "use the file itself").unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument, "{path}");
            assert_eq!(error.message, format!("{path} is a shortcut; use the file itself"));
        }
        assert!(refuse_shortcut(Path::new("C:\\Data\\link.txt"), "use the file itself").is_ok());
        assert!(refuse_shortcut(Path::new("C:\\Tools\\app.exe"), "use the file itself").is_ok());
    }
}
