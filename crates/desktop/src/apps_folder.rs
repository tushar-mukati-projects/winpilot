use std::{collections::HashSet, ffi::c_void};

use deckhand_core::{Context, Result};
use windows::{
    Win32::{
        System::Com::{CoTaskMemFree, IBindCtx},
        UI::Shell::{
            BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT, SHGetKnownFolderItem,
            SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
        },
    },
    core::PWSTR,
};

use crate::apps::{AppEntry, AppKind};

const BATCH_SIZE: usize = 64;

struct CoTaskString(PWSTR);

impl CoTaskString {
    fn read(&self) -> Option<String> {
        if self.0.is_null() {
            return None;
        }
        unsafe { self.0.to_string() }.ok()
    }
}

impl Drop for CoTaskString {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast::<c_void>())) };
    }
}

pub(crate) fn list_apps_folder() -> Result<Vec<AppEntry>> {
    let items = apps_folder_items()?;
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    loop {
        let batch = next_batch(&items)?;
        if batch.is_empty() {
            break;
        }
        let fresh = batch.iter().filter_map(describe).filter(|entry| seen.insert(entry.id.clone()));
        entries.extend(fresh);
    }
    entries.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.clone()));
    Ok(entries)
}

fn apps_folder_items() -> Result<IEnumShellItems> {
    let folder: IShellItem = unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) }
        .context("opening the Apps folder")?;
    unsafe { folder.BindToHandler(None::<&IBindCtx>, &BHID_EnumItems) }.context("listing the Apps folder")
}

fn next_batch(items: &IEnumShellItems) -> Result<Vec<IShellItem>> {
    let mut slots: Vec<Option<IShellItem>> = vec![None; BATCH_SIZE];
    let mut fetched = 0u32;
    unsafe { items.Next(&mut slots, Some(&raw mut fetched)) }.context("reading the Apps folder")?;
    Ok(slots.into_iter().take(fetched as usize).flatten().collect())
}

fn describe(item: &IShellItem) -> Option<AppEntry> {
    let name = display_name(item, SIGDN_NORMALDISPLAY)?;
    let id = display_name(item, SIGDN_PARENTRELATIVEPARSING)?;
    if name.is_empty() || id.is_empty() {
        return None;
    }
    let kind = AppKind::of_id(&id);
    Some(AppEntry { name, id, kind })
}

fn display_name(item: &IShellItem, form: SIGDN) -> Option<String> {
    let raw = unsafe { item.GetDisplayName(form) }.ok()?;
    CoTaskString(raw).read()
}
