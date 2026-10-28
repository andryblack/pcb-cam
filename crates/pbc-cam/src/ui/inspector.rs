//! Right-hand inspector panel.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use pbc_cam_core::{
    LayerGeometry, LayerId, OperationId, OperationKind, SourceFormat, ToolId, Unit,
};

use crate::app::{PbcCam, Selection};
use crate::theme;
use crate::ui::widgets;
use crate::view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectorAction {
    ToggleLayer(LayerId, bool),
    ToggleOperation(OperationId, bool),
    Remove,
    Fit,
    ExportGcode,
    EditOperation,
    SetUnits(Unit),
    AddDrill,
    ImportGerber,
    ImportExcellon,
}

/// Draw the right inspector panel.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::PANEL)
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, theme::BORDER));

    let mut action = None;

    egui::Panel::right("inspector")
        .default_size(292.0)
        .min_size(250.0)
        .max_size(400.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            widgets::panel_header(ui, "Inspector", &mut app.settings.show_inspector);
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    match app.selection {
                        Some(Selection::Layer(id)) => {
                            layer_details(ui, app, id, &mut action);
                        }
                        Some(Selection::Tool(id)) => {
                            tool_details(ui, app, id, &mut action);
                        }
                        Some(Selection::Operation(id)) => {
                            operation_details(ui, app, id, &mut action);
                        }
                        None => project_overview(ui, app, &mut action),
                    }

                    ui.add_space(16.0);
                    widgets::hairline(ui);
                    ui.add_space(10.0);
                    view_settings(ui, app, &mut action);
                });
        });

    match action {
        Some(InspectorAction::ToggleLayer(id, visible)) => {
            app.run_command(
                "layer.set_visible",
                serde_json::json!({"id": id.value(), "visible": visible}),
            );
        }
        Some(InspectorAction::ToggleOperation(id, enabled)) => {
            app.run_command(
                "operation.set_enabled",
                serde_json::json!({"id": id.value(), "enabled": enabled}),
            );
        }
        Some(InspectorAction::Remove) => app.remove_selection(),
        Some(InspectorAction::Fit) => app.fit_view(),
        Some(InspectorAction::ExportGcode) => app.export_gcode_dialog(),
        Some(InspectorAction::EditOperation) => app.open_operation_editor(),
        Some(InspectorAction::SetUnits(units)) => app.set_units(units),
        Some(InspectorAction::AddDrill) => app.add_drill_operation(),
        Some(InspectorAction::ImportGerber) => app.import_gerber_dialog(),
        Some(InspectorAction::ImportExcellon) => app.import_excellon_dialog(),
        None => {}
    }
}

fn project_overview(ui: &mut Ui, app: &PbcCam, action: &mut Option<InspectorAction>) {
    widgets::hint_card(
        ui,
        "Nothing selected",
        "Select a layer, tool or operation to inspect its properties. The canvas shows every visible source layer.",
    );

    ui.add_space(14.0);
    widgets::section_label(ui, "Project");
    ui.add_space(5.0);
    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::field_row(ui, "Name", app.project().name());
        widgets::field_row(ui, "Layers", &app.project().layers().len().to_string());
        widgets::field_row(ui, "Tools", &app.project().tools().len().to_string());
        widgets::field_row(
            ui,
            "Operations",
            &app.project().operations().len().to_string(),
        );
        widgets::field_row(ui, "Objects", &app.project().object_count().to_string());
        if let Some(bounds) = app.project().bounds(true) {
            widgets::field_row(ui, "Extents", &format_bounds(bounds, app.project().units()));
        }
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                vec2((ui.available_width() - 6.0) / 2.0, 25.0),
                Button::new(RichText::new("+ Gerber").size(11.0)),
            )
            .clicked()
        {
            *action = Some(InspectorAction::ImportGerber);
        }
        if ui
            .add_sized(
                vec2((ui.available_width() - 6.0) / 2.0, 25.0),
                Button::new(RichText::new("+ Excellon").size(11.0)),
            )
            .clicked()
        {
            *action = Some(InspectorAction::ImportExcellon);
        }
    });
}

fn layer_details(ui: &mut Ui, app: &PbcCam, id: LayerId, action: &mut Option<InspectorAction>) {
    let Some(layer) = app.project().layer(id).cloned() else {
        return;
    };
    let mut visible = layer.visible();

    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(layer.name())
                    .size(13.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::pill(ui, layer.format().label(), theme::ACCENT);
            });
        });
        ui.add_space(8.0);

        if ui.checkbox(&mut visible, "Visible").changed() {
            *action = Some(InspectorAction::ToggleLayer(id, visible));
        }
        widgets::field_row(ui, "Objects", &layer.object_count().to_string());
        widgets::field_row(
            ui,
            "Source",
            &layer
                .source()
                .and_then(|path| path.file_name())
                .map_or_else(
                    || "Imported from memory".to_owned(),
                    |name| name.to_string_lossy().into_owned(),
                ),
        );
        if let Some(bounds) = layer.bounds() {
            widgets::field_row(ui, "Min", &format_point(bounds.min, app.project().units()));
            widgets::field_row(ui, "Max", &format_point(bounds.max, app.project().units()));
        }
        widgets::field_row(ui, "Warnings", &layer.warnings().len().to_string());
    });

    if let Some(warning) = layer.warnings().first() {
        ui.add_space(8.0);
        ui.label(RichText::new(warning).size(10.0).color(theme::COPPER));
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button("Fit project").clicked() {
            *action = Some(InspectorAction::Fit);
        }
        if matches!(layer.format(), SourceFormat::Excellon)
            && ui.button("+ Drill operation").clicked()
        {
            *action = Some(InspectorAction::AddDrill);
        }
    });
    ui.add_space(4.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Remove layer").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::Remove);
    }
}

fn tool_details(ui: &mut Ui, app: &PbcCam, id: ToolId, action: &mut Option<InspectorAction>) {
    let Some(tool) = app.project().tool(id).cloned() else {
        return;
    };
    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tool.name())
                    .size(13.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::pill(ui, tool.kind().label(), theme::ACCENT);
            });
        });
        ui.add_space(8.0);
        widgets::field_row(ui, "Diameter", &format!("{:.3} mm", tool.diameter_mm()));
        widgets::field_row(ui, "Feed", &format!("{:.0} mm/min", tool.feed_mm_min()));
        widgets::field_row(ui, "Plunge", &format!("{:.0} mm/min", tool.plunge_mm_min()));
        widgets::field_row(ui, "Spindle", &format!("{} rpm", tool.spindle_rpm()));
        if let Some(layer) = tool.source_layer().and_then(|id| app.project().layer(id)) {
            widgets::field_row(ui, "Source layer", layer.name());
        }
    });

    ui.add_space(10.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Remove tool").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::Remove);
    }
}

fn operation_details(
    ui: &mut Ui,
    app: &PbcCam,
    id: OperationId,
    action: &mut Option<InspectorAction>,
) {
    let Some(operation) = app.project().operation(id).cloned() else {
        return;
    };
    let mut enabled = operation.enabled();
    let OperationKind::Drill(drill) = operation.kind();
    let layer = app.project().layer(drill.layer);
    let (holes, slots) = layer
        .and_then(|layer| match layer.geometry() {
            LayerGeometry::Excellon(excellon) => Some((excellon.holes.len(), excellon.slots.len())),
            _ => None,
        })
        .unwrap_or((0, 0));

    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(operation.name())
                    .size(13.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::pill(ui, "Drill", theme::COPPER);
            });
        });
        ui.add_space(8.0);

        if ui.checkbox(&mut enabled, "Enabled").changed() {
            *action = Some(InspectorAction::ToggleOperation(id, enabled));
        }
        widgets::field_row(ui, "Layer", layer.map_or("Missing", |layer| layer.name()));
        widgets::field_row(ui, "Holes", &holes.to_string());
        widgets::field_row(ui, "Slots", &slots.to_string());
        ui.add_space(6.0);
        widgets::field_row(ui, "Depth", &format!("{:.2} mm", drill.depth_mm));
        widgets::field_row(ui, "Retract", &format!("{:.2} mm", drill.retract_mm));
        widgets::field_row(ui, "Feed", &format!("{:.0} mm/min", drill.feed_mm_min));
        widgets::field_row(ui, "Plunge", &format!("{:.0} mm/min", drill.plunge_mm_min));
        widgets::field_row(ui, "Dwell", &format!("{:.2} s", drill.dwell_s));
        widgets::field_row(ui, "Spindle", &format!("{} rpm", drill.spindle_rpm));
    });

    ui.add_space(10.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Edit parameters…").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::EditOperation);
    }
    ui.add_space(4.0);
    let export = ui.add_enabled(
        enabled,
        Button::new(RichText::new("Export G-code…").size(11.0)),
    );
    if export.clicked() {
        *action = Some(InspectorAction::ExportGcode);
    }
    if !enabled {
        export.on_disabled_hover_text("Enable the operation first");
    }
    ui.add_space(4.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Remove operation").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::Remove);
    }
}

fn view_settings(ui: &mut Ui, app: &mut PbcCam, action: &mut Option<InspectorAction>) {
    widgets::section_label(ui, "View");
    ui.add_space(5.0);
    ui.checkbox(&mut app.settings.show_grid, "Show grid");
    ui.checkbox(&mut app.settings.show_origin, "Origin and axes");

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Units").size(11.0).color(theme::TEXT_MUTED));
        let current = app.project().units();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(Button::selectable(current == Unit::Inch, "in"))
                .clicked()
            {
                *action = Some(InspectorAction::SetUnits(Unit::Inch));
            }
            if ui
                .add(Button::selectable(current == Unit::Millimeter, "mm"))
                .clicked()
            {
                *action = Some(InspectorAction::SetUnits(Unit::Millimeter));
            }
        });
    });

    ui.add_space(6.0);
    field_value(ui, "Zoom", &format!("{:.0} %", app.view.zoom_percent()));
    field_value(
        ui,
        "Grid",
        &view::format_length(
            view::grid_step_mm(app.view.zoom()) as f32,
            app.project().units(),
        ),
    );
    ui.add_space(8.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Fit project").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::Fit);
    }
}

fn field_value(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(theme::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(value)
                        .size(11.0)
                        .monospace()
                        .color(theme::TEXT),
                )
                .truncate(),
            );
        });
    });
}

fn format_point(point: pbc_cam_core::Point, units: Unit) -> String {
    format!(
        "X {:.3}  Y {:.3} {}",
        units.from_mm(point.x),
        units.from_mm(point.y),
        units.symbol()
    )
}

fn format_bounds(bounds: pbc_cam_core::Bounds, units: Unit) -> String {
    format!(
        "{:.2} × {:.2} {}",
        units.from_mm(bounds.width()),
        units.from_mm(bounds.height()),
        units.symbol()
    )
}
