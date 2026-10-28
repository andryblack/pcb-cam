//! Application state and top-level egui layout.

use eframe::egui::{self, Align2, Key, Modifiers, RichText, Stroke, Vec2};
use pbc_cam_core::{LayerId, Project, Unit};

use crate::theme;
use crate::ui;
use crate::ui::widgets;
use crate::view::ViewState;

/// Product name used in the title bar and dialogs.
pub(crate) const APP_NAME: &str = "PBC-CAM";
/// Window title.
pub(crate) const WINDOW_TITLE: &str = "PBC-CAM — PCB Fabrication CAM";
/// Application version.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shortcut labels shown in menus.
pub(crate) const SHORTCUT_NEW: &str = if cfg!(target_os = "macos") {
    "Cmd+N"
} else {
    "Ctrl+N"
};
pub(crate) const SHORTCUT_QUIT: &str = if cfg!(target_os = "macos") {
    "Cmd+Q"
} else {
    "Ctrl+Q"
};

/// Toggles that control the workspace chrome.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ViewSettings {
    pub(crate) show_grid: bool,
    pub(crate) show_origin: bool,
    pub(crate) show_project_panel: bool,
    pub(crate) show_inspector: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_origin: true,
            show_project_panel: true,
            show_inspector: true,
        }
    }
}

/// Top-level application state.
#[derive(Debug)]
pub struct PbcCam {
    pub(crate) project: Project,
    pub(crate) view: ViewState,
    pub(crate) settings: ViewSettings,
    pub(crate) selected_layer: Option<LayerId>,
    pub(crate) status: String,
    pub(crate) about_open: bool,
    pub(crate) shortcuts_open: bool,
}

impl PbcCam {
    /// Build the application and install the PBC-CAM style.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        log::info!("starting {APP_NAME} {VERSION}");

        Self {
            project: Project::default(),
            view: ViewState::default(),
            settings: ViewSettings::default(),
            selected_layer: None,
            status: "Ready".to_owned(),
            about_open: false,
            shortcuts_open: false,
        }
    }

    pub(crate) fn new_project(&mut self) {
        self.project = Project::default();
        self.selected_layer = None;
        self.view.reset();
        self.status = "New project created".to_owned();
        log::info!("new project created");
    }

    pub(crate) fn reset_view(&mut self) {
        self.view.reset();
        self.status = "View reset".to_owned();
    }

    pub(crate) fn set_units(&mut self, units: Unit) {
        if self.project.units() != units {
            self.project.set_units(units);
            self.status = format!("Display units: {}", units.name());
        }
    }

    pub(crate) fn quit(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.about_open = false;
            self.shortcuts_open = false;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q)) {
            Self::quit(ctx);
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.about_open = true;
            return;
        }

        if self.about_open || self.shortcuts_open {
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::N)) {
            self.new_project();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F)) {
            self.reset_view();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::G)) {
            self.settings.show_grid = !self.settings.show_grid;
        }
    }

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        if self.about_open {
            let mut open = true;
            let mut close_clicked = false;

            egui::Window::new(format!("About {APP_NAME}"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(390.0)
                .frame(dialog_frame())
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        widgets::brand_mark(ui, 58.0);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(APP_NAME).size(23.0).strong());
                            ui.label(
                                RichText::new("PCB fabrication CAM")
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });

                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(
                            "Cross-platform CAM workbench for turning Gerber and \
                             Excellon fabrication data into CNC control programs.",
                        )
                        .size(12.5)
                        .color(theme::TEXT),
                    );
                    ui.add_space(10.0);

                    widgets::field_row(ui, "Version", VERSION);
                    widgets::field_row(ui, "License", "MIT");
                    widgets::field_row(ui, "Built with", "Rust + egui");
                    widgets::field_row(ui, "Milestone", "Application shell");

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("© 2026 andryblack")
                                .size(10.5)
                                .color(theme::TEXT_FAINT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                close_clicked = true;
                            }
                        });
                    });
                });

            self.about_open = open && !close_clicked;
        }

        if self.shortcuts_open {
            let mut open = true;
            let mut close_clicked = false;

            egui::Window::new("Keyboard shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(360.0)
                .frame(dialog_frame())
                .show(ctx, |ui| {
                    shortcut_section(ui, "Project");
                    shortcut_row(ui, SHORTCUT_NEW, "New project");
                    shortcut_row(ui, SHORTCUT_QUIT, "Quit");
                    shortcut_row(ui, "F1", "About");

                    ui.add_space(10.0);
                    shortcut_section(ui, "Canvas");
                    shortcut_row(ui, "Scroll", "Zoom around pointer");
                    shortcut_row(ui, "Drag", "Pan");
                    shortcut_row(ui, "Double-click", "Reset view");
                    shortcut_row(ui, "F", "Reset view");
                    shortcut_row(ui, "G", "Toggle grid");
                    shortcut_row(ui, "Esc", "Close dialog");

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close_clicked = true;
                        }
                    });
                });

            self.shortcuts_open = open && !close_clicked;
        }
    }
}

impl eframe::App for PbcCam {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());

        ui::menu::show(ui, self);
        ui::toolbar::show(ui, self);
        ui::status_bar::show(ui, self);

        if self.settings.show_project_panel {
            ui::project_panel::show(ui, self);
        }
        if self.settings.show_inspector {
            ui::inspector::show(ui, self);
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui::viewport::show(ui, self);
        });

        let ctx = ui.ctx().clone();
        self.show_dialogs(&ctx);
    }
}

fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL_RAISED)
        .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(18))
}

fn shortcut_section(ui: &mut egui::Ui, title: &str) {
    widgets::section_label(ui, title);
    ui.add_space(3.0);
}

fn shortcut_row(ui: &mut egui::Ui, keys: &str, action: &str) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(theme::CANVAS)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(keys)
                        .size(10.5)
                        .monospace()
                        .color(theme::COPPER),
                );
            });
        ui.label(RichText::new(action).size(11.5).color(theme::TEXT_MUTED));
    });
}
