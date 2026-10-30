//! PCB-CAM desktop application entry point.

#![forbid(unsafe_code)]
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;
mod licenses;
mod preview;
mod theme;
mod ui;
mod view;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let sample = args.iter().any(|arg| arg == "--sample" || arg == "--demo");
    let files: Vec<PathBuf> = args
        .into_iter()
        .filter(|arg| !arg.starts_with('-'))
        .map(PathBuf::from)
        .collect();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(app::WINDOW_TITLE)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([960.0, 600.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ui::widgets::ICON_PNG) {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        app::APP_NAME,
        native_options,
        Box::new(move |cc| {
            let mut app = app::PbcCam::new(cc);
            if sample {
                app.load_demo();
            } else {
                for file in files {
                    app.import_path(file);
                }
            }
            Ok(Box::new(app))
        }),
    )
}
