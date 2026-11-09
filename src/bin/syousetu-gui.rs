#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
//! Thin native desktop entry point.
#![forbid(unsafe_code)]
mod desktop;
fn main() -> eframe::Result {
    desktop::run()
}
