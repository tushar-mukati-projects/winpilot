use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, ERROR_FILE_NOT_FOUND, ERROR_NO_ASSOCIATION, ERROR_PATH_NOT_FOUND},
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Threading::GetProcessId,
        },
        UI::Shell::{
            AO_NONE, ApplicationActivationManager, IApplicationActivationManager, SEE_MASK_FLAG_NO_UI,
            SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
        },
    },
    core::{HSTRING, PCWSTR},
};
use winpilot_core::{Context, Error, ErrorCode, Result};

use crate::{
    activation::Activation,
    handle::{adopt, raw},
};

pub(crate) struct ShellRequest<'a> {
    pub(crate) file: &'a str,
    pub(crate) verb: Option<&'a str>,
    pub(crate) args: Option<&'a str>,
    pub(crate) activation: Activation,
}

pub(crate) fn activate_packaged(aumid: &str, args: Option<&str>) -> Result<u32> {
    let manager: IApplicationActivationManager =
        unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_ALL) }
            .context("creating the application activation manager")?;
    let aumid_text = HSTRING::from(aumid);
    let args_text = HSTRING::from(args.unwrap_or_default());
    unsafe { manager.ActivateApplication(PCWSTR(aumid_text.as_ptr()), PCWSTR(args_text.as_ptr()), AO_NONE) }
        .map_err(|error| launch_error(&error, aumid))
}

pub(crate) fn shell_execute(shell: &ShellRequest<'_>) -> Result<Option<u32>> {
    let file_text = HSTRING::from(shell.file);
    let verb_text = shell.verb.map(HSTRING::from);
    let args_text = shell.args.map(HSTRING::from);
    let mut request = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: optional_text(verb_text.as_ref()),
        lpFile: PCWSTR(file_text.as_ptr()),
        lpParameters: optional_text(args_text.as_ref()),
        nShow: shell.activation.launch_command().0,
        ..SHELLEXECUTEINFOW::default()
    };
    unsafe { ShellExecuteExW(&raw mut request) }.map_err(|error| launch_error(&error, shell.file))?;
    Ok(launched_pid(&request))
}

fn optional_text(text: Option<&HSTRING>) -> PCWSTR {
    text.map_or(PCWSTR::null(), |text| PCWSTR(text.as_ptr()))
}

fn launched_pid(request: &SHELLEXECUTEINFOW) -> Option<u32> {
    let process = adopt(request.hProcess)?;
    let pid = unsafe { GetProcessId(raw(&process)) };
    (pid != 0).then_some(pid)
}

fn launch_error(error: &windows::core::Error, target: &str) -> Error {
    let code = error.code();
    let missing = [ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_NO_ASSOCIATION]
        .iter()
        .any(|missing| missing.to_hresult() == code);
    let kind = if missing {
        ErrorCode::NotFound
    } else if code == ERROR_CANCELLED.to_hresult() {
        ErrorCode::Cancelled
    } else {
        ErrorCode::Internal
    };
    Error::new(kind, format!("launching {target}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_maps_to_not_found() {
        let error = windows::core::Error::from_hresult(ERROR_FILE_NOT_FOUND.to_hresult());
        assert_eq!(launch_error(&error, "x").code, ErrorCode::NotFound);
        let cancelled = windows::core::Error::from_hresult(ERROR_CANCELLED.to_hresult());
        assert_eq!(launch_error(&cancelled, "x").code, ErrorCode::Cancelled);
    }
}
