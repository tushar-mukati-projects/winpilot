use winpilot_core::{Error, Result};

use crate::languages::language_index;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub translate: bool,
    pub threads: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedOptions {
    pub language: Option<usize>,
    pub translate: bool,
    pub threads: usize,
}

impl TranscribeOptions {
    pub(crate) fn resolve(&self, logical_cores: usize) -> Result<ResolvedOptions> {
        Ok(ResolvedOptions {
            language: resolve_language(self.language.as_deref())?,
            translate: self.translate,
            threads: resolve_threads(self.threads, logical_cores)?,
        })
    }
}

pub fn default_threads(logical_cores: usize) -> usize {
    (logical_cores / 2).max(1)
}

pub(crate) fn logical_cores() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
}

fn resolve_language(language: Option<&str>) -> Result<Option<usize>> {
    let Some(raw) = language else { return Ok(None) };
    let name = raw.trim().to_ascii_lowercase();
    if name.is_empty() || name == "auto" {
        return Ok(None);
    }
    language_index(&name)
        .map(Some)
        .ok_or_else(|| Error::invalid(format!("unknown language '{raw}'; use a code such as en or hi, or auto")))
}

fn resolve_threads(threads: Option<usize>, logical_cores: usize) -> Result<usize> {
    let cores = logical_cores.max(1);
    match threads {
        None => Ok(default_threads(cores)),
        Some(count) if (1..=cores).contains(&count) => Ok(count),
        Some(count) => Err(Error::invalid(format!("threads must be between 1 and {cores}, not {count}"))),
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    fn options(language: Option<&str>, threads: Option<usize>) -> TranscribeOptions {
        TranscribeOptions { language: language.map(str::to_string), translate: false, threads }
    }

    #[test]
    fn auto_and_missing_language_detect() {
        for language in [None, Some("auto"), Some(" AUTO "), Some("")] {
            assert_eq!(options(language, None).resolve(8).unwrap().language, None);
        }
    }

    #[test]
    fn hindi_and_english_resolve_by_code_or_name() {
        assert_eq!(options(Some("hi"), None).resolve(8).unwrap().language, Some(17));
        assert_eq!(options(Some("Hindi"), None).resolve(8).unwrap().language, Some(17));
        assert_eq!(options(Some("EN"), None).resolve(8).unwrap().language, Some(0));
    }

    #[test]
    fn unknown_language_is_invalid() {
        let error = options(Some("xx"), None).resolve(8).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.contains("'xx'"));
    }

    #[test]
    fn language_with_nul_is_invalid() {
        assert!(options(Some("en\0"), None).resolve(8).is_err());
    }

    #[test]
    fn threads_default_to_half_the_cores() {
        assert_eq!(options(None, None).resolve(16).unwrap().threads, 8);
        assert_eq!(options(None, None).resolve(1).unwrap().threads, 1);
        assert_eq!(options(None, None).resolve(0).unwrap().threads, 1);
        assert_eq!(default_threads(7), 3);
    }

    #[test]
    fn threads_must_fit_the_machine() {
        assert_eq!(options(None, Some(16)).resolve(16).unwrap().threads, 16);
        assert!(options(None, Some(0)).resolve(16).is_err());
        assert!(options(None, Some(17)).resolve(16).is_err());
    }

    #[test]
    fn translate_is_carried_through() {
        let resolved = TranscribeOptions { translate: true, ..TranscribeOptions::default() }.resolve(4).unwrap();
        assert!(resolved.translate);
    }
}
