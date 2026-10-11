use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    FILEOPENDIALOGOPTIONS, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT,
    FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, IShellItemArray,
    SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{IDCANCEL, IDOK, MB_OKCANCEL, MessageBoxW};
use windows::core::{Error, HRESULT, PWSTR, Result, w};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice<T> {
    Chosen(T),
    Cancelled,
}

const CANCELLED: HRESULT = HRESULT::from_win32(ERROR_CANCELLED.0);

pub fn pick_files_to_open(owner: HWND) -> Result<Choice<Vec<String>>> {
    let dialog: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)? };
    add_options(&dialog, FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)?;
    if !accepted(&dialog, owner)? {
        return Ok(Choice::Cancelled);
    }
    let items = unsafe { dialog.GetResults()? };
    Ok(Choice::Chosen(paths_of(&items)?))
}

pub fn pick_save_path(owner: HWND) -> Result<Choice<String>> {
    let dialog: IFileSaveDialog = unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)? };
    add_options(&dialog, FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM)?;
    unsafe { dialog.SetFileName(w!("report.txt"))? };
    if !accepted(&dialog, owner)? {
        return Ok(Choice::Cancelled);
    }
    let item = unsafe { dialog.GetResult()? };
    Ok(Choice::Chosen(file_system_path(&item)?))
}

pub fn ask_confirmation(owner: HWND) -> Result<bool> {
    match unsafe { MessageBoxW(Some(owner), w!("Are you sure?"), w!("Deckhand Fixture Confirm"), MB_OKCANCEL) } {
        IDOK => Ok(true),
        IDCANCEL => Ok(false),
        _ => Err(Error::from_thread()),
    }
}

fn add_options(dialog: &IFileDialog, extra: FILEOPENDIALOGOPTIONS) -> Result<()> {
    let current = unsafe { dialog.GetOptions()? };
    unsafe { dialog.SetOptions(current | extra) }
}

fn accepted(dialog: &IFileDialog, owner: HWND) -> Result<bool> {
    match unsafe { dialog.Show(Some(owner)) } {
        Ok(()) => Ok(true),
        Err(error) if error.code() == CANCELLED => Ok(false),
        Err(error) => Err(error),
    }
}

fn paths_of(items: &IShellItemArray) -> Result<Vec<String>> {
    let count = unsafe { items.GetCount()? };
    (0..count).map(|index| file_system_path(&unsafe { items.GetItemAt(index)? })).collect()
}

fn file_system_path(item: &IShellItem) -> Result<String> {
    let name = TaskMemoryText(unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? });
    Ok(name.to_string_lossy())
}

struct TaskMemoryText(PWSTR);

impl TaskMemoryText {
    fn to_string_lossy(&self) -> String {
        if self.0.is_null() {
            return String::new();
        }
        String::from_utf16_lossy(unsafe { self.0.as_wide() })
    }
}

impl Drop for TaskMemoryText {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast())) };
    }
}
