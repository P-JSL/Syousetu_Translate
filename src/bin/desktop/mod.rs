//! Desktop state and entry point; views and task orchestration are separate.
mod app;
mod font;
mod learning;
mod novel;
mod settings;
mod tasks;
mod widgets;
use eframe::egui;
use syousetu_translate::{config::Config, jobs::Queue, learning::Memory};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Novel,
    Settings,
    Learning,
}

struct Desktop {
    tab: Tab,
    config: Config,
    config_path: String,
    queue: Queue,
    selected: Option<u64>,
    info_job: Option<u64>,
    url: String,
    output: String,
    dictionary: String,
    start: usize,
    end: usize,
    metadata: String,
    message: String,
    memory: Memory,
    source: String,
    previous: String,
    corrected: String,
    term_mode: bool,
}

pub(super) fn run() -> eframe::Result {
    eframe::run_native(
        "Syousetu Translate",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([860.0, 850.0])
                .with_min_inner_size([600.0, 500.0]),
            ..Default::default()
        },
        Box::new(|creation| Ok(Box::new(Desktop::new(&creation.egui_ctx)))),
    )
}

#[cfg(test)]
mod tests;
