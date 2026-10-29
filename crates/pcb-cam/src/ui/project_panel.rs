//! Browser panel: project summary, source layers, source tools and machining tools.

use eframe::egui::{
    self, Align, Button, Color32, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2,
};

use crate::app::{PbcCam, Selection};
use crate::theme;
use crate::ui::widgets;
use pcb_cam_core::LayerGeometry;

/// Draw the left browser panel.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::left("project_panel")
        .default_size(268.0)
        .min_size(232.0)
        .max_size(390.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            widgets::panel_header(ui, "Browser", &mut app.settings.show_project_panel);
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt("browser_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    project_card(ui, app);
                    ui.add_space(14.0);

                    section_header(ui, "Layers", app.project().layers().len());
                    ui.add_space(4.0);
                    layer_list(ui, app);
                    ui.add_space(8.0);
                    import_buttons(ui, app);

                    if source_tools_visible(app) {
                        ui.add_space(16.0);
                        widgets::hairline(ui);
                        ui.add_space(10.0);
                        source_tools_section(ui, app);
                    }

                    ui.add_space(16.0);
                    widgets::hairline(ui);
                    ui.add_space(10.0);
                    section_header(ui, "Machining tools", app.session.tool_library().len());
                    ui.add_space(4.0);
                    tool_list(ui, app);

                    ui.add_space(16.0);
                    widgets::hairline(ui);
                    ui.add_space(10.0);
                    output_section(ui, app);
                });
        });
}

fn project_card(ui: &mut Ui, app: &PbcCam) {
    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            widgets::brand_mark(ui, 36.0);
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(app.project().name())
                        .size(14.0)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!(
                        "{} task{}",
                        app.project().tasks().len(),
                        if app.project().tasks().len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .size(10.5)
                    .color(theme::text_muted()),
                );
            });
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            stat(ui, "LAYERS", &app.project().layers().len().to_string());
            ui.add_space(20.0);
            stat(
                ui,
                "MACH. TOOLS",
                &app.session.tool_library().len().to_string(),
            );
            ui.add_space(20.0);
            stat(ui, "OBJECTS", &app.project().object_count().to_string());
        });
    });
}

fn stat(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(label)
                .size(8.8)
                .strong()
                .color(theme::text_faint()),
        );
        ui.label(
            RichText::new(value)
                .size(12.0)
                .strong()
                .color(theme::text()),
        );
    });
}

fn section_header(ui: &mut Ui, title: &str, count: usize) {
    ui.horizontal(|ui| {
        widgets::section_label(ui, title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::pill(ui, &count.to_string(), theme::text_muted());
        });
    });
}

fn layer_list(ui: &mut Ui, app: &mut PbcCam) {
    if app.project().layers().is_empty() {
        widgets::hint_card(
            ui,
            "No source layers",
            "Import a Gerber copper, outline or silkscreen layer, or an Excellon drill file.",
        );
        return;
    }

    let mut visibility_change = None;
    let mut selection = None;

    for layer in app.project().layers() {
        let selected = app.selection == Some(Selection::Layer(layer.id()));
        let mut visible = layer.visible();

        ui.horizontal(|ui| {
            if ui
                .add(egui::Checkbox::new(&mut visible, ""))
                .on_hover_text("Toggle visibility")
                .changed()
            {
                visibility_change = Some((layer.id(), visible));
            }

            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().circle_filled(
                dot_rect.center(),
                3.5,
                if visible {
                    color(layer.color())
                } else {
                    theme::text_faint()
                },
            );

            let name = RichText::new(layer.name()).size(12.0).color(if selected {
                theme::copper_bright()
            } else {
                theme::text()
            });
            if ui.selectable_label(selected, name).clicked() {
                selection = Some(Selection::Layer(layer.id()));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(layer.format().label())
                        .size(9.0)
                        .color(theme::text_faint()),
                );
                ui.label(
                    RichText::new(layer.object_count().to_string())
                        .size(9.5)
                        .monospace()
                        .color(theme::text_muted()),
                );
            });
        });
    }

    if let Some((id, visible)) = visibility_change {
        app.run_command(
            "layer.set_visible",
            serde_json::json!({"id": id.value(), "visible": visible}),
        );
    }
    if let Some(selection) = selection {
        app.selection = Some(selection);
    }
}

fn import_buttons(ui: &mut Ui, app: &mut PbcCam) {
    if ui
        .add_sized(
            egui::vec2(ui.available_width(), 25.0),
            Button::new(RichText::new("+ Import").size(11.0)),
        )
        .clicked()
    {
        app.import_dialog();
    }
}

fn source_tools_visible(app: &PbcCam) -> bool {
    let layer_id = match app.selection {
        Some(Selection::Layer(id)) => Some(id),
        Some(Selection::Task(id)) => app.project().task(id).map(|task| task.source_layer()),
        _ => None,
    };
    layer_id
        .and_then(|id| app.project().layer(id))
        .is_some_and(|layer| matches!(layer.geometry(), LayerGeometry::Excellon(_)))
}

fn source_tools_section(ui: &mut Ui, app: &mut PbcCam) {
    let layer_id = match app.selection {
        Some(Selection::Layer(id)) => Some(id),
        Some(Selection::Task(id)) => app.project().task(id).map(|task| task.source_layer()),
        _ => None,
    };
    let Some(layer) = layer_id.and_then(|id| app.project().layer(id)) else {
        return;
    };
    if !matches!(layer.geometry(), LayerGeometry::Excellon(_)) {
        return;
    }
    let tools = layer.source_tools();
    section_header(ui, "Source tools", tools.len());
    ui.add_space(4.0);
    if tools.is_empty() {
        widgets::hint_card(
            ui,
            "No source tools",
            "This drill file has no tool metadata.",
        );
        return;
    }
    for tool in &tools {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&tool.label)
                    .size(10.5)
                    .monospace()
                    .color(theme::text_muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let add = ui
                    .add(
                        Button::new(RichText::new("+").size(12.0)).min_size(egui::vec2(18.0, 16.0)),
                    )
                    .on_hover_text("Create a machining tool from this source tool");
                ui.label(
                    RichText::new(format!("{} obj", tool.count))
                        .size(9.5)
                        .color(theme::text_faint()),
                );
                if add.clicked() {
                    app.add_tool_from_source(tool);
                }
            });
        });
    }
    ui.label(
        RichText::new(
            "Read-only. Use + to add a matching machining tool; these entries also filter drilling, hole-milling and spot-drilling tasks.",
        )
        .size(9.0)
        .color(theme::text_faint()),
    );
}

fn tool_list(ui: &mut Ui, app: &mut PbcCam) {
    if ui
        .add_sized(
            egui::vec2(ui.available_width(), 24.0),
            Button::new(RichText::new("+ Machining tool").size(10.5)),
        )
        .clicked()
    {
        app.add_tool();
    }
    ui.add_space(4.0);

    if app.session.tool_library().is_empty() {
        widgets::hint_card(
            ui,
            "No machining tools",
            "Add drills, end mills or V-bits to the user library.",
        );
        return;
    }

    let mut selection = None;
    for tool in app.session.tool_library().tools() {
        let selected = app.selection == Some(Selection::Tool(tool.id()));
        ui.horizontal(|ui| {
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 3.0, theme::accent());
            let name = RichText::new(tool.name()).size(11.5).color(if selected {
                theme::copper_bright()
            } else {
                theme::text()
            });
            if ui.selectable_label(selected, name).clicked() {
                selection = Some(Selection::Tool(tool.id()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Ø {:.2}", tool.diameter_mm()))
                        .size(9.5)
                        .monospace()
                        .color(theme::text_muted()),
                );
            });
        });
    }
    if let Some(selection) = selection {
        app.selection = Some(selection);
    }
}

fn output_section(ui: &mut Ui, app: &mut PbcCam) {
    let enabled = app.project().enabled_task_count();
    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new("NC program")
                .size(12.0)
                .strong()
                .color(theme::text()),
        );
        ui.add_space(2.0);
        if enabled == 0 {
            ui.label(
                RichText::new("Add and enable a task to generate a combined G-code program.")
                    .size(10.5)
                    .color(theme::text_faint()),
            );
        } else {
            ui.label(
                RichText::new(format!(
                    "{enabled} enabled task{} ready",
                    if enabled == 1 { "" } else { "s" }
                ))
                .size(10.5)
                .color(theme::text_muted()),
            );
            ui.add_space(6.0);
            let export = ui.add_sized(
                egui::vec2(ui.available_width(), 25.0),
                Button::new(RichText::new("Preview all G-code…").size(11.0)),
            );
            if export.clicked() {
                app.open_all_gcode_preview();
            }
        }
    });
}

fn color([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}
