//! Application menu bar.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui};

use crate::app::{self, PbcCam};
use crate::theme;

/// Draw the native-looking menu bar at the top of the window.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::APP_BG)
        .inner_margin(Margin::symmetric(10, 3))
        .stroke(Stroke::new(1.0, theme::BORDER));

    egui::Panel::top("menu_bar")
        .exact_size(31.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add(Button::new("New Project").shortcut_text(app::SHORTCUT_NEW))
                        .clicked()
                    {
                        app.new_project();
                        ui.close();
                    }

                    ui.separator();

                    ui.add_enabled(false, Button::new("Open Gerber…"))
                        .on_disabled_hover_text("Gerber import is the next milestone");
                    ui.add_enabled(false, Button::new("Open Excellon…"))
                        .on_disabled_hover_text("Excellon import is the next milestone");

                    ui.separator();

                    if ui
                        .add(Button::new("Quit").shortcut_text(app::SHORTCUT_QUIT))
                        .clicked()
                    {
                        PbcCam::quit(ui.ctx());
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut app.settings.show_project_panel, "Project panel");
                    ui.checkbox(&mut app.settings.show_inspector, "Inspector");
                    ui.separator();
                    ui.checkbox(&mut app.settings.show_grid, "Grid");
                    ui.checkbox(&mut app.settings.show_origin, "Origin and axes");
                    ui.separator();
                    if ui
                        .add(Button::new("Reset view").shortcut_text("F"))
                        .clicked()
                    {
                        app.reset_view();
                        ui.close();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        app.shortcuts_open = true;
                        ui.close();
                    }
                    if ui.button("About PBC-CAM").clicked() {
                        app.about_open = true;
                        ui.close();
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("v{}  /  UI preview", app::VERSION))
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        });
}
