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

                    let selected_task = matches!(app.selection, Some(Selection::Task(_)));
                    let export_task = ui.add_enabled(
                        selected_task,
                        Button::new("Export Selected Task G-code…").shortcut_text("Cmd/Ctrl+E"),
                    );
                    if export_task.clicked() {
                        ui.close();
                        app.export_task_gcode_dialog();
                    }
                    if !selected_task {
                        export_task.on_disabled_hover_text("Select a task first");
                    }

                    let all_tasks = app.project().enabled_task_count() > 0;
                    let export_all = ui.add_enabled(
                        all_tasks,
                        Button::new("Export All Tasks G-code…").shortcut_text("Cmd/Ctrl+Shift+E"),
                    );
                    if export_all.clicked() {
                        ui.close();
                        app.export_all_gcode_dialog();
                    }
                    if !all_tasks {
                        export_all.on_disabled_hover_text("Add and enable a task first");
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

                ui.menu_button("Tasks", |ui| {
                    let can_drill = app.can_add_drill_task();
                    let drill = ui.add_enabled(can_drill, Button::new("Add Drilling Task"));
                    if drill.clicked() {
                        app.add_drill_task();
                        ui.close();
                    }
                    if !can_drill {
                        drill.on_disabled_hover_text(
                            "Import an Excellon layer with holes or slots first",
                        );
                    }

                    let can_cutout = app.can_add_cutout_task();
                    let cutout = ui.add_enabled(can_cutout, Button::new("Add Contour Cut Task"));
                    if cutout.clicked() {
                        app.add_cutout_task();
                        ui.close();
                    }
                    if !can_cutout {
                        cutout.on_disabled_hover_text(
                            "Import a Gerber layer with closed contours first",
                        );
                    }

                    let can_engrave = app.can_add_engrave_task();
                    let engrave = ui.add_enabled(can_engrave, Button::new("Add Engraving Task"));
                    if engrave.clicked() {
                        app.add_engrave_task();
                        ui.close();
                    }
                    if !can_engrave {
                        engrave.on_disabled_hover_text("Import a Gerber layer with paths first");
                    }

                    let selected_task = matches!(app.selection, Some(Selection::Task(_)));
                    ui.separator();
                    let edit = ui.add_enabled(selected_task, Button::new("Edit Selected Task…"));
                    if edit.clicked() {
                        app.open_task_editor();
                        ui.close();
                    }

                    ui.separator();
                    let export_task =
                        ui.add_enabled(selected_task, Button::new("Export Selected Task G-code…"));
                    if export_task.clicked() {
                        ui.close();
                        app.export_task_gcode_dialog();
                    }
                    let export_all = ui.add_enabled(
                        app.project().enabled_task_count() > 0,
                        Button::new("Export All Tasks G-code…"),
                    );
                    if export_all.clicked() {
                        ui.close();
                        app.export_all_gcode_dialog();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut app.settings.show_project_panel, "Browser");
                    ui.checkbox(&mut app.settings.show_inspector, "Inspector");
                    ui.checkbox(&mut app.settings.show_timeline, "Task timeline");
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
                        RichText::new(format!("v{}  /  Milestone 3", app::VERSION))
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        });
}
