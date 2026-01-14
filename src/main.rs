#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod app;
mod data;

use app::VisualizerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "csvpshmem visualizer",
        options,
        Box::new(|cc| Ok(Box::new(VisualizerApp::new(cc)))),
    )
}
