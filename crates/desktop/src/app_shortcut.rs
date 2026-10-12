use std::path::{Path, PathBuf};

use deckhand_core::{Context, Result};
use windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        UI::Shell::{IShellItem2, SHCreateItemFromParsingName},
    },
    core::{GUID, HSTRING, PCWSTR},
};

use crate::{apps::APPS_FOLDER_PREFIX, co_task_string::CoTaskString};

const LINK_TARGET: PROPERTYKEY =
    PROPERTYKEY { fmtid: GUID::from_u128(0xb9b4_b3fc_2b51_4a42_b5d8_3241_46af_cf25), pid: 2 };
const LINK_ARGUMENTS: PROPERTYKEY =
    PROPERTYKEY { fmtid: GUID::from_u128(0x436f_2667_14e2_4feb_b30a_146c_53b5_b674), pid: 100 };
const PROGRAM_EXTENSIONS: [&str; 4] = ["exe", "com", "bat", "cmd"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Shortcut {
    pub(crate) target: Option<String>,
    pub(crate) arguments: Option<String>,
}

impl Shortcut {
    pub(crate) fn of_app(app_id: &str, app_name: &str) -> Result<Self> {
        let item = apps_folder_item(app_id).context(&format!("reading the Start menu entry of {app_name}"))?;
        Ok(Self { target: text_property(&item, &LINK_TARGET), arguments: text_property(&item, &LINK_ARGUMENTS) })
    }

    pub(crate) fn program(&self) -> Option<PathBuf> {
        let path = Path::new(self.target.as_deref()?.trim());
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        (path.is_absolute() && PROGRAM_EXTENSIONS.contains(&extension.as_str())).then(|| path.to_path_buf())
    }

    pub(crate) fn command_line(&self, args: &str) -> String {
        match self.arguments.as_deref().map(str::trim).filter(|own| !own.is_empty()) {
            Some(own) => format!("{own} {args}"),
            None => args.to_string(),
        }
    }
}

fn apps_folder_item(app_id: &str) -> windows::core::Result<IShellItem2> {
    let path = HSTRING::from(format!("{APPS_FOLDER_PREFIX}{app_id}"));
    unsafe { SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None) }
}

fn text_property(item: &IShellItem2, key: &PROPERTYKEY) -> Option<String> {
    let text = unsafe { item.GetString(key) }.ok()?;
    CoTaskString(text).read().filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(target: &str, arguments: Option<&str>) -> Shortcut {
        Shortcut { target: Some(target.to_string()), arguments: arguments.map(str::to_string) }
    }

    #[test]
    fn programs_are_absolute_paths_to_runnable_files() {
        let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        assert_eq!(shortcut(chrome, None).program(), Some(PathBuf::from(chrome)));
        assert!(shortcut(r"C:\Tools\RUN.CMD", None).program().is_some());
        assert!(shortcut(r"C:\Tools\setup.bat", None).program().is_some());
    }

    #[test]
    fn shell_places_documents_and_relative_paths_are_not_programs() {
        assert_eq!(shortcut("::{52205FD8-5DFB-447D-801A-D0B52F2E83E1}", None).program(), None);
        assert_eq!(shortcut(r"C:\Docs\manual.pdf", None).program(), None);
        assert_eq!(shortcut(r"tools\app.exe", None).program(), None);
        assert_eq!(shortcut(r"C:\Tools\app", None).program(), None);
        assert_eq!(Shortcut::default().program(), None);
    }

    #[test]
    fn the_entrys_own_arguments_come_first() {
        let profile = shortcut("chrome.exe", Some(r#"--profile-directory="Profile 2""#));
        assert_eq!(
            profile.command_line("https://example.com/a"),
            r#"--profile-directory="Profile 2" https://example.com/a"#
        );
        assert_eq!(shortcut("code.exe", None).command_line(r"C:\src"), r"C:\src");
        assert_eq!(shortcut("code.exe", Some("  ")).command_line(r"C:\src"), r"C:\src");
    }
}
