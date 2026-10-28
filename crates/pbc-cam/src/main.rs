//! PBC-CAM desktop application entry point.

#![forbid(unsafe_code)]
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;
mod theme;
mod ui;
mod view;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png"))
        .expect("the bundled window icon must be a valid PNG");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(app::WINDOW_TITLE)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(icon),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        app::APP_NAME,
        native_options,
        Box::new(|cc| Ok(Box::new(app::PbcCam::new(cc)))),
    )
}
