//! Right-hand inspector panel.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use pbc_cam_core::{
    ContourSide, EngraveSide, LayerGeometry, LayerId, OperationId, OperationKind, Tool, ToolId,
    Unit,
};

use crate::app::{PbcCam, Selection, TaskDraftKind};
use crate::theme;
use crate::ui::widgets;
use crate::view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectorAction {
    ToggleLayer(LayerId, bool),
    ToggleTask(OperationId, bool),
    Remove,
    Fit,
    ExportTask,
    EditTool,
    SetUnits(Unit),
    MoveTask(OperationId, usize),
    AddDrill,
    AddCutout,
    AddEngrave,
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
    app.sync_task_draft();

    egui::Panel::right("inspector")
        .default_size(300.0)
        .min_size(255.0)
        .max_size(410.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            widgets::panel_header(ui, "Inspector", &mut app.settings.show_inspector);
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt("inspector_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    match app.selection {
                        Some(Selection::Layer(id)) => {
                            layer_details(ui, app, id, &mut action);
                        }
                        Some(Selection::Tool(id)) => {
                            tool_details(ui, app, id, &mut action);
                        }
                        Some(Selection::Task(id)) => {
                            task_details(ui, app, id, &mut action);
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
        Some(InspectorAction::ToggleTask(id, enabled)) => {
            app.run_command(
                "task.set_enabled",
                serde_json::json!({"id": id.value(), "enabled": enabled}),
            );
        }
        Some(InspectorAction::Remove) => app.remove_selection(),
        Some(InspectorAction::Fit) => app.fit_view(),
        Some(InspectorAction::ExportTask) => app.export_task_gcode_dialog(),
        Some(InspectorAction::EditTool) => app.open_tool_editor(),
        Some(InspectorAction::SetUnits(units)) => app.set_units(units),
        Some(InspectorAction::MoveTask(id, target)) => {
            app.run_command(
                "task.move",
                serde_json::json!({"id": id.value(), "to": target}),
            );
        }
        Some(InspectorAction::AddDrill) => app.add_drill_task(),
        Some(InspectorAction::AddCutout) => app.add_cutout_task(),
        Some(InspectorAction::AddEngrave) => app.add_engrave_task(),
        Some(InspectorAction::ImportGerber) => app.import_gerber_dialog(),
        Some(InspectorAction::ImportExcellon) => app.import_excellon_dialog(),
        None => {}
    }
}

fn project_overview(ui: &mut Ui, app: &PbcCam, action: &mut Option<InspectorAction>) {
    widgets::hint_card(
        ui,
        "Nothing selected",
        "Select a layer, tool or task to inspect its properties. The canvas shows every visible source layer.",
    );

    ui.add_space(14.0);
    widgets::section_label(ui, "Project");
    ui.add_space(5.0);
    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::field_row(ui, "Name", app.project().name());
        widgets::field_row(ui, "Layers", &app.project().layers().len().to_string());
        widgets::field_row(
            ui,
            "Machining tools",
            &app.session.tool_library().len().to_string(),
        );
        widgets::field_row(ui, "Tasks", &app.project().tasks().len().to_string());
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

    let source_tools = layer.source_tools();
    ui.add_space(10.0);
    widgets::section_label(ui, "Source tools");
    ui.add_space(3.0);
    if source_tools.is_empty() {
        ui.label(
            RichText::new("No aperture/tool metadata in this layer.")
                .size(10.0)
                .color(theme::TEXT_FAINT),
        );
    } else {
        for tool in &source_tools {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&tool.label)
                        .size(10.5)
                        .monospace()
                        .color(theme::TEXT_MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} obj", tool.count))
                            .size(9.5)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        }
        ui.label(
            RichText::new("Read-only. Excellon tools filter drilling tasks.")
                .size(9.0)
                .color(theme::TEXT_FAINT),
        );
    }

    ui.add_space(10.0);
    if ui.button("Fit project").clicked() {
        *action = Some(InspectorAction::Fit);
    }

    match layer.geometry() {
        LayerGeometry::Excellon(_) => {
            if ui.button("+ Drilling task").clicked() {
                *action = Some(InspectorAction::AddDrill);
            }
        }
        LayerGeometry::Gerber(gerber) => {
            if gerber.has_closed_contours() && ui.button("+ Contour cut task").clicked() {
                *action = Some(InspectorAction::AddCutout);
            }
            if gerber.has_engraving_paths() && ui.button("+ Engraving task").clicked() {
                *action = Some(InspectorAction::AddEngrave);
            }
        }
    }

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
    let Some(tool) = app.session.tool_library().tool(id).cloned() else {
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
    });

    ui.add_space(10.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Edit tool…").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::EditTool);
    }
    ui.add_space(4.0);
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

fn task_details(
    ui: &mut Ui,
    app: &mut PbcCam,
    id: OperationId,
    action: &mut Option<InspectorAction>,
) {
    let Some(task) = app.project().task(id).cloned() else {
        return;
    };
    let mut enabled = task.enabled();
    let layer_name = app.project().layer(task.source_layer()).map_or_else(
        || "Missing layer".to_owned(),
        |layer| layer.name().to_owned(),
    );
    let tools = app.session.tool_library().tools().to_vec();
    let source_tools: Vec<pbc_cam_core::SourceToolInfo> = app
        .project()
        .layer(task.source_layer())
        .map(|layer| layer.source_tools())
        .unwrap_or_default();
    let selected_counts = match task.kind() {
        OperationKind::Drill(drill) => filtered_counts(app, &task, &drill.source_tools),
        _ => (0, 0),
    };
    let Some(draft) = app.task_draft.as_mut() else {
        return;
    };
    let mut commit = false;

    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Task").size(13.0).strong().color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::pill(ui, task.kind_label(), task_color(task.kind()));
            });
        });
        ui.add_space(8.0);

        if ui.checkbox(&mut enabled, "Enabled").changed() {
            *action = Some(InspectorAction::ToggleTask(id, enabled));
        }

        ui.add_space(4.0);
        ui.label(RichText::new("Name").size(10.5).color(theme::TEXT_MUTED));
        let name = ui.add(
            egui::TextEdit::singleline(&mut draft.name)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Small),
        );
        if name.lost_focus() && name.changed() {
            commit = true;
        }

        ui.add_space(6.0);
        widgets::field_row(ui, "Source", &layer_name);

        ui.add_space(4.0);
        ui.label(
            RichText::new("Machining tool")
                .size(10.5)
                .color(theme::TEXT_MUTED),
        );
        if tool_selector(ui, &tools, &mut draft.tool) {
            commit = true;
        }

        match &mut draft.kind {
            TaskDraftKind::Drill {
                source_tools: selected,
                depth,
                retract,
                feed,
                plunge,
                dwell,
                stepdown,
                spindle,
            } => {
                ui.add_space(8.0);
                widgets::section_label(ui, "Source holes");
                ui.add_space(3.0);
                if source_tools.is_empty() {
                    ui.label(
                        RichText::new("The source layer has no tools.")
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                } else {
                    for info in &source_tools {
                        if let Some(number) = info.number {
                            let mut checked = selected.contains(&number);
                            if ui
                                .checkbox(
                                    &mut checked,
                                    format!("{}  ({} obj)", info.label, info.count),
                                )
                                .changed()
                            {
                                if checked {
                                    selected.insert(number);
                                } else {
                                    selected.remove(&number);
                                }
                                commit = true;
                            }
                        }
                    }
                    if selected.is_empty() {
                        ui.label(
                            RichText::new("Select at least one source tool.")
                                .size(10.0)
                                .color(theme::ERROR),
                        );
                    }
                }
                let (holes, slots) = selected_counts;
                widgets::field_row(
                    ui,
                    "Selected",
                    &format!(
                        "{} hole{} · {} slot{}",
                        holes,
                        if holes == 1 { "" } else { "s" },
                        slots,
                        if slots == 1 { "" } else { "s" }
                    ),
                );

                ui.add_space(8.0);
                widgets::section_label(ui, "Cut parameters");
                ui.add_space(3.0);
                commit |= inline_f64(ui, "Depth", depth, 0.01..=50.0, " mm");
                commit |= inline_f64(ui, "Retract", retract, 0.1..=100.0, " mm");
                commit |= inline_f64(ui, "Feed", feed, 1.0..=10_000.0, " mm/min");
                commit |= inline_f64(ui, "Plunge", plunge, 1.0..=5_000.0, " mm/min");
                commit |= inline_f64(ui, "Dwell", dwell, 0.0..=10.0, " s");
                commit |= inline_f64(ui, "Helical step", stepdown, 0.05..=10.0, " mm");
                commit |= inline_u32(ui, "Spindle", spindle, 0..=60_000, " rpm");
            }
            TaskDraftKind::Cutout {
                side,
                depth,
                retract,
                feed,
                plunge,
                spindle,
            } => {
                ui.add_space(8.0);
                widgets::section_label(ui, "Cutter");
                ui.add_space(3.0);
                let mut changed_side = false;
                ui.horizontal(|ui| {
                    for candidate in [
                        ContourSide::Outside,
                        ContourSide::Inside,
                        ContourSide::OnLine,
                    ] {
                        if ui
                            .add(Button::selectable(*side == candidate, candidate.label()))
                            .clicked()
                        {
                            *side = candidate;
                            changed_side = true;
                        }
                    }
                });
                commit |= changed_side;

                ui.add_space(8.0);
                widgets::section_label(ui, "Cut parameters");
                ui.add_space(3.0);
                commit |= inline_f64(ui, "Depth", depth, 0.01..=50.0, " mm");
                commit |= inline_f64(ui, "Retract", retract, 0.1..=100.0, " mm");
                commit |= inline_f64(ui, "Feed", feed, 1.0..=10_000.0, " mm/min");
                commit |= inline_f64(ui, "Plunge", plunge, 1.0..=5_000.0, " mm/min");
                commit |= inline_u32(ui, "Spindle", spindle, 0..=60_000, " rpm");
            }
            TaskDraftKind::Engrave {
                side,
                offset,
                passes,
                stepover,
                depth,
                retract,
                feed,
                plunge,
                spindle,
            } => {
                ui.add_space(8.0);
                widgets::section_label(ui, "Offset passes");
                ui.add_space(3.0);
                let mut changed_side = false;
                ui.horizontal(|ui| {
                    for candidate in [EngraveSide::Outside, EngraveSide::Inside] {
                        if ui
                            .add(Button::selectable(*side == candidate, candidate.label()))
                            .clicked()
                        {
                            *side = candidate;
                            changed_side = true;
                        }
                    }
                });
                commit |= changed_side;
                commit |= inline_f64(ui, "Offset", offset, 0.0..=50.0, " mm");
                commit |= inline_u32(ui, "Passes", passes, 1..=100, "");
                commit |= inline_f64(ui, "Offset step", stepover, 0.0..=50.0, " mm");
                ui.label(
                    RichText::new(
                        "Offset passes apply to closed contours; open paths are engraved once.",
                    )
                    .size(9.5)
                    .color(theme::TEXT_FAINT),
                );

                ui.add_space(8.0);
                widgets::section_label(ui, "Cut parameters");
                ui.add_space(3.0);
                commit |= inline_f64(ui, "Depth", depth, 0.01..=50.0, " mm");
                commit |= inline_f64(ui, "Retract", retract, 0.1..=100.0, " mm");
                commit |= inline_f64(ui, "Feed", feed, 1.0..=10_000.0, " mm/min");
                commit |= inline_f64(ui, "Plunge", plunge, 1.0..=5_000.0, " mm/min");
                commit |= inline_u32(ui, "Spindle", spindle, 0..=60_000, " rpm");
            }
        }
    });

    if commit {
        app.commit_task_draft();
    }

    ui.add_space(10.0);
    let export = ui.add_enabled(
        enabled,
        Button::new(RichText::new("Export task G-code…").size(11.0)),
    );
    if export.clicked() {
        *action = Some(InspectorAction::ExportTask);
    }
    if !enabled {
        export.on_disabled_hover_text("Enable the task first");
    }

    ui.add_space(4.0);
    if ui
        .add_sized(
            vec2(ui.available_width(), 26.0),
            Button::new(RichText::new("Remove task").size(11.0)),
        )
        .clicked()
    {
        *action = Some(InspectorAction::Remove);
    }

    let index = app
        .project()
        .tasks()
        .iter()
        .position(|candidate| candidate.id() == id)
        .unwrap_or(0);
    let count = app.project().tasks().len();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let earlier = ui.add_enabled(
            index > 0,
            Button::new(RichText::new("← Earlier").size(10.5)),
        );
        if earlier.clicked() {
            *action = Some(InspectorAction::MoveTask(id, index.saturating_sub(1)));
        }
        let later = ui.add_enabled(
            index + 1 < count,
            Button::new(RichText::new("Later →").size(10.5)),
        );
        if later.clicked() {
            *action = Some(InspectorAction::MoveTask(id, index + 1));
        }
    });
}

fn tool_selector(ui: &mut Ui, tools: &[Tool], selected: &mut ToolId) -> bool {
    let selected_text = tools
        .iter()
        .find(|tool| tool.id() == *selected)
        .map_or("Missing tool".to_owned(), |tool| {
            format!("{} · Ø{:.2} mm", tool.name(), tool.diameter_mm())
        });
    let before = *selected;
    egui::ComboBox::from_id_salt("inspector_task_tool")
        .selected_text(selected_text)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for tool in tools {
                ui.selectable_value(
                    selected,
                    tool.id(),
                    format!(
                        "{} · {} · Ø{:.2} mm",
                        tool.name(),
                        tool.kind().label(),
                        tool.diameter_mm()
                    ),
                );
            }
        });
    *selected != before
}

fn inline_f64(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
) -> bool {
    let mut commit = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(theme::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let response = ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(0.05)
                    .max_decimals(3)
                    .suffix(suffix),
            );
            commit = response.drag_stopped()
                || (response.changed() && response.lost_focus())
                || (response.changed()
                    && response.hovered()
                    && ui.input(|input| input.pointer.any_released()));
        });
    });
    commit
}

fn inline_u32(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
) -> bool {
    let mut commit = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(theme::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let response = ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(100)
                    .suffix(suffix),
            );
            commit = response.drag_stopped()
                || (response.changed() && response.lost_focus())
                || (response.changed()
                    && response.hovered()
                    && ui.input(|input| input.pointer.any_released()));
        });
    });
    commit
}

fn filtered_counts(
    app: &PbcCam,
    task: &pbc_cam_core::Task,
    selected: &std::collections::BTreeSet<u32>,
) -> (usize, usize) {
    let Some(layer) = app.project().layer(task.source_layer()) else {
        return (0, 0);
    };
    let LayerGeometry::Excellon(excellon) = layer.geometry() else {
        return (0, 0);
    };
    let include = |tool: u32| selected.contains(&tool);
    (
        excellon
            .holes
            .iter()
            .filter(|hole| include(hole.tool))
            .count(),
        excellon
            .slots
            .iter()
            .filter(|slot| include(slot.tool))
            .count(),
    )
}

fn task_color(kind: &OperationKind) -> egui::Color32 {
    match kind {
        OperationKind::Drill(_) => theme::COPPER,
        OperationKind::Cutout(_) => theme::ACCENT,
        OperationKind::Engrave(_) => theme::COPPER_BRIGHT,
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
