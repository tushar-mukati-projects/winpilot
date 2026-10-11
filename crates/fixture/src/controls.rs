use std::ptr;

use windows::Win32::Foundation::{E_FAIL, HINSTANCE, HWND};
use windows::Win32::System::SystemServices::{SS_ENDELLIPSIS, SS_LEFT, SS_NOPREFIX};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_AUTOCHECKBOX, BS_PUSHBUTTON, CB_ADDSTRING, CBS_DROPDOWNLIST, CreateWindowExW, ES_AUTOHSCROLL, ES_AUTOVSCROLL,
    ES_LEFT, ES_MULTILINE, ES_WANTRETURN, HMENU, LB_ADDSTRING, LBS_NOTIFY, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_CHILD, WS_EX_CLIENTEDGE, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{Error, HSTRING, PCWSTR, Result, w};

use crate::child::{item, send};
use crate::font::GuiFont;
use crate::ids::{COLOUR_COMBO, ITEM_LIST};
use crate::layout::{CONTROLS, ControlKind, ControlSpec};
use crate::state;
use crate::text::{COLOURS, ITEM_COUNT, item_label};

struct Look {
    class: PCWSTR,
    style: u32,
    extended: WINDOW_EX_STYLE,
}

const PLAIN: WINDOW_EX_STYLE = WINDOW_EX_STYLE(0);
const STATIC_TEXT: u32 = SS_LEFT.0 | SS_NOPREFIX.0;
const FOCUSABLE: u32 = WS_TABSTOP.0;

fn look_of(kind: ControlKind) -> Look {
    match kind {
        ControlKind::Label => Look { class: w!("STATIC"), style: STATIC_TEXT, extended: PLAIN },
        ControlKind::Status => Look { class: w!("STATIC"), style: STATIC_TEXT | SS_ENDELLIPSIS.0, extended: PLAIN },
        ControlKind::SingleLineEdit => {
            Look { class: w!("EDIT"), style: FOCUSABLE | (ES_LEFT | ES_AUTOHSCROLL) as u32, extended: WS_EX_CLIENTEDGE }
        }
        ControlKind::MultiLineEdit => Look {
            class: w!("EDIT"),
            style: FOCUSABLE | WS_VSCROLL.0 | (ES_LEFT | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32,
            extended: WS_EX_CLIENTEDGE,
        },
        ControlKind::PushButton => {
            Look { class: w!("BUTTON"), style: FOCUSABLE | BS_PUSHBUTTON as u32, extended: PLAIN }
        }
        ControlKind::CheckBox => {
            Look { class: w!("BUTTON"), style: FOCUSABLE | BS_AUTOCHECKBOX as u32, extended: PLAIN }
        }
        ControlKind::DropDownList => {
            Look { class: w!("COMBOBOX"), style: FOCUSABLE | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32, extended: PLAIN }
        }
        ControlKind::ListBox => Look {
            class: w!("LISTBOX"),
            style: FOCUSABLE | WS_VSCROLL.0 | LBS_NOTIFY as u32,
            extended: WS_EX_CLIENTEDGE,
        },
    }
}

pub fn create_all(window: HWND, instance: HINSTANCE) -> Result<()> {
    for control in &CONTROLS {
        create(window, instance, control)?;
    }
    fill_colours(window)?;
    fill_items(window)
}

fn create(window: HWND, instance: HINSTANCE, control: &ControlSpec) -> Result<HWND> {
    let look = look_of(control.kind);
    let visibility = if control.initially_visible { WS_VISIBLE.0 } else { 0 };
    let style = WINDOW_STYLE(WS_CHILD.0 | visibility | look.style);
    let id = HMENU(ptr::without_provenance_mut(control.id as usize));
    let text = HSTRING::from(control.text);
    unsafe {
        CreateWindowExW(
            look.extended,
            look.class,
            &text,
            style,
            0,
            0,
            0,
            0,
            Some(window),
            Some(id),
            Some(instance),
            None,
        )
    }
}

fn fill_colours(window: HWND) -> Result<()> {
    let combo = item(window, COLOUR_COMBO)?;
    COLOURS.iter().try_for_each(|name| add_entry(combo, CB_ADDSTRING, name))
}

fn fill_items(window: HWND) -> Result<()> {
    let list = item(window, ITEM_LIST)?;
    (1..=ITEM_COUNT).try_for_each(|number| add_entry(list, LB_ADDSTRING, &item_label(number)))
}

fn add_entry(control: HWND, message: u32, entry: &str) -> Result<()> {
    let entry = HSTRING::from(entry);
    if send(control, message, 0, entry.as_ptr().addr() as isize) < 0 {
        return Err(Error::new(E_FAIL, "adding a list entry failed"));
    }
    Ok(())
}

pub fn fit_to_dpi(window: HWND, dpi: u32) -> Result<()> {
    apply_font(window, dpi)?;
    arrange(window, dpi)
}

fn apply_font(window: HWND, dpi: u32) -> Result<()> {
    let font = GuiFont::for_dpi(dpi)?;
    for control in &CONTROLS {
        send(item(window, control.id)?, WM_SETFONT, font.handle().0.addr(), 1);
    }
    state::adopt_font(font);
    Ok(())
}

fn arrange(window: HWND, dpi: u32) -> Result<()> {
    for control in &CONTROLS {
        let bounds = control.bounds.scaled(dpi);
        let child = item(window, control.id)?;
        unsafe {
            SetWindowPos(child, None, bounds.x, bounds.y, bounds.width, bounds.height, SWP_NOZORDER | SWP_NOACTIVATE)?;
        }
    }
    Ok(())
}
