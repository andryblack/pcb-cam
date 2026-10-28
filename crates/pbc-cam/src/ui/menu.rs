//! Application menu bar.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui};

use crate::app::{self, PbcCam, Selection};
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

                    if ui
                        .add(Button::new("Import Gerber…").shortcut_text(app::SHORTCUT_OPEN))
                        .clicked()
                    {
                        ui.close();
                        app.import_gerber_dialog();
                    }
                    if ui
                        .add(Button::new("Import Excellon…").shortcut_text("Cmd/Ctrl+Shift+O"))
                        .clicked()
                    {
                        ui.close();
                        app.import_excellon_dialog();
                    }
                    if ui.button("Load Demo Project").clicked() {
                        app.load_demo();
                        ui.close();
                    }

                    ui.separator();

                    let can_export = app.project().enabled_drill_count() > 0;
                    let export = ui.add_enabled(
                        can_export,
                        Button::new("Export G-code…").shortcut_text("Cmd/Ctrl+E"),
                    );
                    if export.clicked() {
                        ui.close();
                        app.export_gcode_dialog();
                    }
                    if !can_export {
                        export.on_disabled_hover_text("Add and enable a drilling operation first");
                    }

                    ui.separator();

                    if ui
                        .add(Button::new("Quit").shortcut_text(app::SHORTCUT_QUIT))
                        .clicked()
                    {
                        PbcCam::quit(ui.ctx());
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let undo = ui.add_enabled(
                        app.session.can_undo(),
                        Button::new("Undo").shortcut_text(app::SHORTCUT_UNDO),
                    );
                    if undo.clicked() {
                        app.undo();
                        ui.close();
                    }
                    let redo = ui.add_enabled(
                        app.session.can_redo(),
                        Button::new("Redo").shortcut_text(app::SHORTCUT_REDO),
                    );
                    if redo.clicked() {
                        app.redo();
                        ui.close();
                    }
                    ui.separator();
                    let remove = ui.add_enabled(
                        app.selection.is_some(),
                        Button::new("Remove selected").shortcut_text("Delete"),
                    );
                    if remove.clicked() {
                        app.remove_selection();
                        ui.close();
                    }
                });

                ui.menu_button("Toolpaths", |ui| {
                    let can_add = app.can_add_drill_operation();
                    let add = ui.add_enabled(can_add, Button::new("Add Drilling Operation"));
                    if add.clicked() {
                        app.add_drill_operation();
                        ui.close();
                    }
                    if !can_add {
                        add.on_disabled_hover_text(
                            "Import an Excellon layer with holes or slots first",
                        );
                    }
                    let edit = ui.add_enabled(
                        matches!(app.selection, Some(Selection::Operation(_))),
                        Button::new("Edit Operation Parameters…"),
                    );
                    if edit.clicked() {
                        app.open_operation_editor();
                        ui.close();
                    }
                    ui.separator();
                    let export = ui.add_enabled(
                        app.project().enabled_drill_count() > 0,
                        Button::new("Export G-code…").shortcut_text("Cmd/Ctrl+E"),
                    );
                    if export.clicked() {
                        ui.close();
                        app.export_gcode_dialog();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut app.settings.show_project_panel, "Browser");
                    ui.checkbox(&mut app.settings.show_inspector, "Inspector");
                    ui.checkbox(&mut app.settings.show_timeline, "Operation timeline");
                    ui.separator();
                    ui.checkbox(&mut app.settings.show_grid, "Grid");
                    ui.checkbox(&mut app.settings.show_origin, "Origin and axes");
                    ui.separator();
                    if ui.add(Button::new("Fit view").shortcut_text("F")).clicked() {
                        app.fit_view();
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
                        RichText::new(format!("v{}  /  Milestone 2", app::VERSION))
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        });
}
