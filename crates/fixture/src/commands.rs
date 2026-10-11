use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::BST_CHECKED;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BN_CLICKED, CB_GETCURSEL, CBN_SELCHANGE, LB_GETCURSEL, LBN_SELCHANGE,
};
use windows::core::Result;

use crate::appearance;
use crate::child::{index_from, item, send, set_text};
use crate::dialogs;
use crate::ids::{
    APPEARED_BUTTON, CLICK_BUTTON, CLICK_STATUS, COLOUR_COMBO, COLOUR_STATUS, CONFIRM_BUTTON, CONFIRM_STATUS,
    FEATURE_CHECKBOX, FEATURE_STATUS, ITEM_LIST, ITEM_STATUS, KEY_STATUS, OPEN_BUTTON, OPEN_STATUS, SAVE_BUTTON,
    SAVE_STATUS, SHOW_LATER_BUTTON,
};
use crate::params::{high_word, low_word};
use crate::shortcuts::Shortcut;
use crate::state;
use crate::text;

pub fn on_command(window: HWND, wparam: WPARAM, lparam: LPARAM) -> Result<()> {
    let id = low_word(wparam.0);
    if lparam.0 == 0 {
        return on_shortcut(window, id);
    }
    on_control(window, i32::from(id), u32::from(high_word(wparam.0)))
}

fn on_shortcut(window: HWND, command: u16) -> Result<()> {
    let Some(shortcut) = Shortcut::from_command(command) else {
        return Ok(());
    };
    set_text(window, KEY_STATUS, &text::last_key(shortcut))
}

fn on_control(window: HWND, id: i32, notification: u32) -> Result<()> {
    match (id, notification) {
        (CLICK_BUTTON, BN_CLICKED) => count_click(window),
        (FEATURE_CHECKBOX, BN_CLICKED) => show_feature(window),
        (COLOUR_COMBO, CBN_SELCHANGE) => show_colour(window),
        (ITEM_LIST, LBN_SELCHANGE) => show_item(window),
        (OPEN_BUTTON, BN_CLICKED) => open_files(window),
        (SAVE_BUTTON, BN_CLICKED) => save_file(window),
        (SHOW_LATER_BUTTON, BN_CLICKED) => appearance::schedule(window),
        (APPEARED_BUTTON, BN_CLICKED) => appearance::acknowledge(window),
        (CONFIRM_BUTTON, BN_CLICKED) => confirm(window),
        _ => Ok(()),
    }
}

fn count_click(window: HWND) -> Result<()> {
    set_text(window, CLICK_STATUS, &text::clicks(state::record_click()))
}

fn show_feature(window: HWND) -> Result<()> {
    let checked = send(item(window, FEATURE_CHECKBOX)?, BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize;
    set_text(window, FEATURE_STATUS, text::feature(checked))
}

fn show_colour(window: HWND) -> Result<()> {
    let index = index_from(send(item(window, COLOUR_COMBO)?, CB_GETCURSEL, 0, 0));
    set_text(window, COLOUR_STATUS, &text::colour(index))
}

fn show_item(window: HWND) -> Result<()> {
    let index = index_from(send(item(window, ITEM_LIST)?, LB_GETCURSEL, 0, 0));
    set_text(window, ITEM_STATUS, &text::selected_item(index))
}

fn open_files(window: HWND) -> Result<()> {
    let outcome = dialogs::pick_files_to_open(window);
    set_text(window, OPEN_STATUS, &text::opened(&outcome))
}

fn save_file(window: HWND) -> Result<()> {
    let outcome = dialogs::pick_save_path(window);
    set_text(window, SAVE_STATUS, &text::saved(&outcome))
}

fn confirm(window: HWND) -> Result<()> {
    let outcome = dialogs::ask_confirmation(window);
    set_text(window, CONFIRM_STATUS, &text::confirmation(&outcome))
}
