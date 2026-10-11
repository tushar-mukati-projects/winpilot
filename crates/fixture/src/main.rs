#![cfg_attr(not(test), windows_subsystem = "windows")]

mod apartment;
mod appearance;
mod child;
mod commands;
mod controls;
mod dialogs;
mod dpi;
mod focus;
mod font;
mod ids;
mod layout;
mod message_loop;
mod options;
mod params;
mod shortcuts;
mod state;
mod text;
mod window;
mod window_proc;

use std::process::ExitCode;

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::Result;

use crate::apartment::StaApartment;
use crate::options::Options;
use crate::shortcuts::Shortcuts;
use crate::state::StateGuard;
use crate::window::{MainWindow, WindowClass};

fn main() -> ExitCode {
    let options = Options::parse(std::env::args_os().skip(1));
    match run(&options) {
        Ok(code) => u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from),
        Err(_) => ExitCode::FAILURE,
    }
}

fn run(options: &Options) -> Result<i32> {
    dpi::become_per_monitor_aware()?;
    let _apartment = StaApartment::enter()?;
    let _state = StateGuard::new();
    let instance = unsafe { GetModuleHandleW(None)? }.into();
    let class = WindowClass::register(instance)?;
    let shortcuts = Shortcuts::create()?;
    let window = MainWindow::create(&class, options)?;
    window.show();
    message_loop::run(window.handle(), &shortcuts)
}
