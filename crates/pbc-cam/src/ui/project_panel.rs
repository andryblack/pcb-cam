//! Browser panel: project summary, source layers and the tool table.

use eframe::egui::{
    self, Align, Button, Color32, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2,
};

use crate::app::{PbcCam, Selection};
use crate::theme;
use crate::ui::widgets;

/// Draw the left browser panel.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::PANEL)
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, theme::BORDER));

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
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    project_card(ui, app);
                    ui.add_space(14.0);

                    section_header(ui, "Layers", app.project().layers().len());
                    ui.add_space(4.0);
                    layer_list(ui, app);
                    ui.add_space(8.0);
                    import_buttons(ui, app);

                    ui.add_space(16.0);
                    widgets::hairline(ui);
                    ui.add_space(10.0);
                    section_header(ui, "Tools", app.project().tools().len());
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
                        .color(theme::TEXT),
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
                    .color(theme::TEXT_MUTED),
                );
            });
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            stat(ui, "LAYERS", &app.project().layers().len().to_string());
            ui.add_space(20.0);
            stat(ui, "TOOLS", &app.project().tools().len().to_string());
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
                .color(theme::TEXT_FAINT),
        );
        ui.label(RichText::new(value).size(12.0).strong().color(theme::TEXT));
    });
}

fn section_header(ui: &mut Ui, title: &str, count: usize) {
    ui.horizontal(|ui| {
        widgets::section_label(ui, title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::pill(ui, &count.to_string(), theme::TEXT_MUTED);
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
                    theme::TEXT_FAINT
                },
            );

            let name = RichText::new(layer.name()).size(12.0).color(if selected {
                theme::COPPER_BRIGHT
            } else {
                theme::TEXT
            });
            if ui.selectable_label(selected, name).clicked() {
                selection = Some(Selection::Layer(layer.id()));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(layer.format().label())
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.label(
                    RichText::new(layer.object_count().to_string())
                        .size(9.5)
                        .monospace()
                        .color(theme::TEXT_MUTED),
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
    let width = (ui.available_width() - 6.0) / 2.0;
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                egui::vec2(width.min(110.0), 25.0),
                Button::new(RichText::new("+ Gerber").size(11.0)),
            )
            .clicked()
        {
            app.import_gerber_dialog();
        }
        if ui
            .add_sized(
                egui::vec2(width.min(110.0), 25.0),
                Button::new(RichText::new("+ Excellon").size(11.0)),
            )
            .clicked()
        {
            app.import_excellon_dialog();
        }
    });
}

fn tool_list(ui: &mut Ui, app: &mut PbcCam) {
    if app.project().tools().is_empty() {
        widgets::hint_card(
            ui,
            "No tools",
            "Drill tools are created automatically when an Excellon file is imported.",
        );
        return;
    }

    let mut selection = None;
    for tool in app.project().tools() {
        let selected = app.selection == Some(Selection::Tool(tool.id()));
        ui.horizontal(|ui| {
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 3.0, theme::ACCENT);
            let name = RichText::new(tool.name()).size(11.5).color(if selected {
                theme::COPPER_BRIGHT
            } else {
                theme::TEXT
            });
            if ui.selectable_label(selected, name).clicked() {
                selection = Some(Selection::Tool(tool.id()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.2} mm", tool.diameter_mm()))
                        .size(9.5)
                        .monospace()
                        .color(theme::TEXT_MUTED),
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
                .color(theme::TEXT),
        );
        ui.add_space(2.0);
        if enabled == 0 {
            ui.label(
                RichText::new("Add and enable a task to generate a combined G-code program.")
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
        } else {
            ui.label(
                RichText::new(format!(
                    "{enabled} enabled task{} ready",
                    if enabled == 1 { "" } else { "s" }
                ))
                .size(10.5)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(6.0);
            let export = ui.add_sized(
                egui::vec2(ui.available_width(), 25.0),
                Button::new(RichText::new("Export all G-code…").size(11.0)),
            );
            if export.clicked() {
                app.export_all_gcode_dialog();
            }
        }
    });
}

fn color([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}
