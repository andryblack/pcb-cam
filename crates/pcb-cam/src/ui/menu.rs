//! Application menu bar.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui};

use crate::app::{self, PbcCam, Selection};
use crate::theme;

/// Draw the native-looking menu bar at the top of the window.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::app_bg())
        .inner_margin(Margin::symmetric(10, 3))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::top("menu_bar")
        .exact_size(31.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                if app.preview.is_some() {
                    preview_menus(ui, app);
                    return;
                }
                ui.menu_button("File", |ui| {
                    if ui
                        .add(Button::new("New Project").shortcut_text(app::SHORTCUT_NEW))
                        .clicked()
                    {
                        app.request_new_project(ui.ctx());
                        ui.close();
                    }

                    if ui
                        .add(Button::new("Open Project…").shortcut_text(app::SHORTCUT_OPEN))
                        .clicked()
                    {
                        ui.close();
                        app.open_project_dialog(ui.ctx());
                    }
                    ui.menu_button("Open Recent", |ui| {
                        let recents = app.recent_projects.clone();
                        if recents.is_empty() {
                            ui.add_enabled(false, Button::new("No recent projects"));
                        } else {
                            for path in recents {
                                let label = path
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| path.display().to_string());
                                if ui
                                    .button(label)
                                    .on_hover_text(path.display().to_string())
                                    .clicked()
                                {
                                    app.request_open_project(path, ui.ctx());
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Clear Recent").clicked() {
                                app.clear_recent_projects();
                            }
                        }
                    });

                    if ui
                        .add(Button::new("Save Project").shortcut_text(app::SHORTCUT_SAVE))
                        .clicked()
                    {
                        ui.close();
                        app.save_project();
                    }
                    if ui
                        .add(Button::new("Save Project As…").shortcut_text(app::SHORTCUT_SAVE_AS))
                        .clicked()
                    {
                        ui.close();
                        app.save_project_as_dialog();
                    }

                    ui.separator();

                    if ui
                        .add(Button::new("Import…").shortcut_text(app::SHORTCUT_IMPORT))
                        .clicked()
                    {
                        ui.close();
                        app.import_dialog();
                    }
                    if ui.button("Load Demo Project").clicked() {
                        app.request_load_demo(ui.ctx());
                        ui.close();
                    }

                    ui.separator();

                    let selected_task = matches!(app.selection, Some(Selection::Task(_)));
                    let export_task = ui.add_enabled(
                        selected_task,
                        Button::new("Preview Selected Task G-code…").shortcut_text("Cmd/Ctrl+E"),
                    );
                    if export_task.clicked() {
                        ui.close();
                        app.open_task_gcode_preview();
                    }
                    if !selected_task {
                        export_task.on_disabled_hover_text("Select a task first");
                    }

                    let all_tasks = app.project().enabled_task_count() > 0;
                    let export_all = ui.add_enabled(
                        all_tasks,
                        Button::new("Preview All Tasks G-code…").shortcut_text("Cmd/Ctrl+Shift+E"),
                    );
                    if export_all.clicked() {
                        ui.close();
                        app.open_all_gcode_preview();
                    }
                    if !all_tasks {
                        export_all.on_disabled_hover_text("Add and enable a task first");
                    }

                    ui.separator();

                    if ui
                        .add(Button::new("Quit").shortcut_text(app::SHORTCUT_QUIT))
                        .clicked()
                    {
                        app.request_quit(ui.ctx());
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

                ui.menu_button("Layer", |ui| {
                    let has_layer = matches!(app.selection, Some(Selection::Layer(_)));
                    ui.add_enabled_ui(has_layer, |ui| {
                        ui.menu_button("Flip Selected Layer", |ui| {
                            flip_entries(ui, app, false);
                        });
                    });
                    ui.add_enabled_ui(!app.project().layers().is_empty(), |ui| {
                        ui.menu_button("Flip All Layers", |ui| {
                            flip_entries(ui, app, true);
                        });
                    });
                    let has_axes = app.project().flip_axes().is_some();
                    let axes_item = ui.add_enabled(has_axes, Button::new("Set Flip Axes…"));
                    if axes_item.clicked() {
                        app.open_flip_axes_dialog();
                        ui.close();
                    }
                    if !has_axes {
                        axes_item.on_disabled_hover_text(
                            "Flip a layer first; the axes are created automatically",
                        );
                    }
                    ui.separator();
                    let remove = ui.add_enabled(
                        has_layer,
                        Button::new("Remove Selected Layer").shortcut_text("Delete"),
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

                    let can_hole_mill = app.can_add_hole_mill_task();
                    let hole_mill =
                        ui.add_enabled(can_hole_mill, Button::new("Add Hole Milling Task"));
                    if hole_mill.clicked() {
                        app.add_hole_mill_task();
                        ui.close();
                    }
                    if !can_hole_mill {
                        hole_mill.on_disabled_hover_text(
                            "Import an Excellon layer with holes or slots first",
                        );
                    }

                    let can_spot = app.can_add_spot_drill_task();
                    let spot = ui.add_enabled(can_spot, Button::new("Add Spot Drill Task"));
                    if spot.clicked() {
                        app.add_spot_drill_task();
                        ui.close();
                    }
                    if !can_spot {
                        spot.on_disabled_hover_text("Import an Excellon layer with holes first");
                    }

                    let can_cutout = app.can_add_cutout_task();
                    let cutout = ui.add_enabled(can_cutout, Button::new("Add Edge Cut Task"));
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
                    let engrave = ui.add_enabled(can_engrave, Button::new("Add Clearance Task"));
                    if engrave.clicked() {
                        app.add_engrave_task();
                        ui.close();
                    }
                    if !can_engrave {
                        engrave.on_disabled_hover_text("Import a Gerber layer with paths first");
                    }

                    let selected_task = matches!(app.selection, Some(Selection::Task(_)));
                    ui.separator();
                    let export_task =
                        ui.add_enabled(selected_task, Button::new("Preview Selected Task G-code…"));
                    if export_task.clicked() {
                        ui.close();
                        app.open_task_gcode_preview();
                    }
                    let export_all = ui.add_enabled(
                        app.project().enabled_task_count() > 0,
                        Button::new("Preview All Tasks G-code…"),
                    );
                    if export_all.clicked() {
                        ui.close();
                        app.open_all_gcode_preview();
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
                    ui.separator();
                    if ui
                        .add(Button::new("Settings…").shortcut_text(app::SHORTCUT_SETTINGS))
                        .clicked()
                    {
                        app.settings_open = true;
                        ui.close();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        app.shortcuts_open = true;
                        ui.close();
                    }
                    if ui.button("About PCB-CAM").clicked() {
                        app.about_open = true;
                        ui.close();
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("v{}  /  Milestone 5", app::VERSION))
                            .size(10.0)
                            .color(theme::text_faint()),
                    );
                });
            });
        });
}

fn preview_menus(ui: &mut egui::Ui, app: &mut PbcCam) {
    ui.menu_button("File", |ui| {
        let export = ui.add(Button::new("Export G-code…").shortcut_text("Cmd/Ctrl+E"));
        if export.clicked() {
            app.export_preview_gcode();
            ui.close();
        }
        if ui
            .add(Button::new("Regenerate Preview").shortcut_text("Cmd/Ctrl+R"))
            .clicked()
        {
            app.regenerate_preview();
            ui.close();
        }
        if ui
            .add(Button::new("Close Preview").shortcut_text("Esc"))
            .clicked()
        {
            app.close_preview();
            ui.close();
        }
        ui.separator();
        if ui
            .add(Button::new("Quit").shortcut_text(app::SHORTCUT_QUIT))
            .clicked()
        {
            app.request_quit(ui.ctx());
        }
    });

    ui.menu_button("View", |ui| {
        if ui.add(Button::new("Fit view").shortcut_text("F")).clicked() {
            if let Some(preview) = &mut app.preview {
                preview.fit_pending = true;
            }
            ui.close();
        }
        ui.separator();
        for preset in [
            crate::preview::CameraPreset::Iso,
            crate::preview::CameraPreset::Top,
            crate::preview::CameraPreset::Front,
        ] {
            if ui.button(format!("{} view", preset.label())).clicked() {
                if let Some(preview) = &mut app.preview {
                    preview.camera.set_preset(preset);
                    preview.fit_pending = true;
                }
                ui.close();
            }
        }
        ui.separator();
        if let Some(preview) = &mut app.preview {
            ui.checkbox(&mut preview.show_cut, "Cutting moves");
            ui.checkbox(&mut preview.show_rapid, "Rapid & travel");
            ui.checkbox(&mut preview.follow, "Follow current command");
            ui.menu_button("Z exaggeration", |ui| {
                for value in [1.0_f32, 2.0, 4.0, 8.0, 16.0] {
                    let selected = (preview.z_scale - value).abs() < f32::EPSILON;
                    if ui.radio(selected, format!("×{value:.0}")).clicked() {
                        preview.z_scale = value;
                        preview.fit_pending = true;
                        ui.close();
                    }
                }
            });
        }
    });

    ui.menu_button("Help", |ui| {
        if ui.button("Keyboard shortcuts").clicked() {
            app.shortcuts_open = true;
            ui.close();
        }
        if ui.button("About PCB-CAM").clicked() {
            app.about_open = true;
            ui.close();
        }
    });

    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(
            RichText::new("G-CODE PREVIEW")
                .size(10.0)
                .color(theme::copper()),
        );
    });
}

fn flip_entries(ui: &mut Ui, app: &mut PbcCam, all: bool) {
    for (label, shortcut, horizontal, vertical) in [
        ("Horizontal (mirror X)", Some("Shift+H"), true, false),
        ("Vertical (mirror Y)", Some("Shift+V"), false, true),
        ("Both (180°)", None, true, true),
    ] {
        let button = match shortcut {
            Some(shortcut) => Button::new(label).shortcut_text(shortcut),
            None => Button::new(label),
        };
        if ui.add(button).clicked() {
            if all {
                app.flip_all_layers(horizontal, vertical);
            } else {
                app.flip_selected_layer(horizontal, vertical);
            }
            ui.close();
        }
    }
}
