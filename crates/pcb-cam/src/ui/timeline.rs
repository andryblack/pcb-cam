//! Task timeline at the bottom of the workspace.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};

use crate::app::{PbcCam, Selection};
use crate::theme;
use crate::ui::widgets;

/// Draw the task timeline above the status bar.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::bottom("task_timeline")
        .exact_size(84.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_label(ui, "Tasks");
                widgets::pill(
                    ui,
                    &app.project().tasks().len().to_string(),
                    theme::text_muted(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Ordered drilling, hole-milling, spot-drilling, edge-cut and clearance steps")
                        .size(10.0)
                        .color(theme::text_faint()),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can_export = app.project().enabled_task_count() > 0;
                    let export = ui.add_enabled(
                        can_export,
                        Button::new(RichText::new("Preview all G-code").size(11.0)),
                    );
                    if export.clicked() {
                        app.open_all_gcode_preview();
                    }
                    if !can_export {
                        export.on_disabled_hover_text("Add and enable a task first");
                    }

                    ui.menu_button(RichText::new("+ Task").size(11.0), |ui| {
                        let drill = ui.add_enabled(
                            app.can_add_drill_task(),
                            Button::new("Drilling"),
                        );
                        if drill.clicked() {
                            app.add_drill_task();
                            ui.close();
                        }
                        let hole_mill = ui.add_enabled(
                            app.can_add_hole_mill_task(),
                            Button::new("Hole milling"),
                        );
                        if hole_mill.clicked() {
                            app.add_hole_mill_task();
                            ui.close();
                        }
                        let spot = ui.add_enabled(
                            app.can_add_spot_drill_task(),
                            Button::new("Spot drill"),
                        );
                        if spot.clicked() {
                            app.add_spot_drill_task();
                            ui.close();
                        }
                        let cutout = ui.add_enabled(
                            app.can_add_cutout_task(),
                            Button::new("Edge cut"),
                        );
                        if cutout.clicked() {
                            app.add_cutout_task();
                            ui.close();
                        }
                        let engrave = ui.add_enabled(
                            app.can_add_engrave_task(),
                            Button::new("Clearance"),
                        );
                        if engrave.clicked() {
                            app.add_engrave_task();
                            ui.close();
                        }
                    });
                });
            });

            ui.add_space(5.0);

            if app.project().tasks().is_empty() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            "No tasks yet — import Gerber/Excellon data, then add a drilling, edge-cut or clearance task.",
                        )
                        .size(11.0)
                        .color(theme::text_faint()),
                    );
                });
                return;
            }

            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut selected_change = None;
                        for (index, task) in app.project().tasks().iter().enumerate() {
                            let selected = app.selection == Some(Selection::Task(task.id()));
                            let color = if task.enabled() {
                                task_color(task.kind())
                            } else {
                                theme::text_faint()
                            };
                            Frame::new()
                                .fill(if selected {
                                    theme::surface_active()
                                } else {
                                    theme::canvas()
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected {
                                        theme::copper_dim()
                                    } else {
                                        theme::border()
                                    },
                                ))
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(9, 5))
                                .show(ui, |ui| {
                                    ui.set_width(190.0);
                                    ui.horizontal(|ui| {
                                        let (dot, _) = ui.allocate_exact_size(
                                            vec2(7.0, 7.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().circle_filled(dot.center(), 3.0, color);
                                        let title = RichText::new(format!(
                                            "{}. {}",
                                            index + 1,
                                            task.name()
                                        ))
                                        .size(11.5)
                                        .strong()
                                        .color(if selected {
                                            theme::copper_bright()
                                        } else {
                                            theme::text()
                                        });
                                        if ui
                                            .add(Button::new(title).frame(false))
                                            .clicked()
                                        {
                                            selected_change =
                                                Some(Selection::Task(task.id()));
                                        }
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(task_subtitle(app, task))
                                                .size(9.5)
                                                .color(theme::text_muted()),
                                        )
                                        .truncate(),
                                    );
                                });
                            ui.add_space(4.0);
                        }
                        if let Some(selection) = selected_change {
                            app.selection = Some(selection);
                        }
                    });
                });
        });
}

fn tool_label(app: &PbcCam, task: &pcb_cam_core::Task) -> String {
    app.session.tool_library().tool(task.tool()).map_or_else(
        || "missing tool".to_owned(),
        |tool| format!("Ø{:.2} {}", tool.diameter_mm(), tool.name()),
    )
}

fn task_color(kind: &pcb_cam_core::OperationKind) -> egui::Color32 {
    match kind {
        pcb_cam_core::OperationKind::Drill(_) => theme::copper(),
        pcb_cam_core::OperationKind::SpotDrill(_) => theme::copper_bright(),
        pcb_cam_core::OperationKind::HoleMill(_) => theme::accent(),
        pcb_cam_core::OperationKind::Cutout(_) => theme::accent(),
        pcb_cam_core::OperationKind::Engrave(_) => theme::copper_bright(),
    }
}

fn task_subtitle(app: &PbcCam, task: &pcb_cam_core::Task) -> String {
    let enabled = if task.enabled() { "" } else { " · disabled" };
    match task.kind() {
        pcb_cam_core::OperationKind::Drill(drill) => {
            let holes = app
                .project()
                .layer(drill.layer)
                .and_then(|layer| match layer.geometry() {
                    pcb_cam_core::LayerGeometry::Excellon(excellon) => Some(
                        excellon
                            .holes
                            .iter()
                            .filter(|hole| {
                                drill.source_tools.is_empty()
                                    || drill.source_tools.contains(&hole.tool)
                            })
                            .count()
                            + excellon
                                .slots
                                .iter()
                                .filter(|slot| {
                                    drill.source_tools.is_empty()
                                        || drill.source_tools.contains(&slot.tool)
                                })
                                .count(),
                    ),
                    pcb_cam_core::LayerGeometry::Gerber(_) => None,
                })
                .unwrap_or(0);
            let tool = tool_label(app, task);
            format!(
                "Drill · {tool} · {holes} objects · {:.2} mm{enabled}",
                drill.depth_mm
            )
        }
        pcb_cam_core::OperationKind::HoleMill(mill) => {
            let objects = app
                .project()
                .layer(mill.layer)
                .and_then(|layer| match layer.geometry() {
                    pcb_cam_core::LayerGeometry::Excellon(excellon) => Some(
                        excellon
                            .holes
                            .iter()
                            .filter(|hole| {
                                mill.source_tools.is_empty()
                                    || mill.source_tools.contains(&hole.tool)
                            })
                            .count()
                            + excellon
                                .slots
                                .iter()
                                .filter(|slot| {
                                    mill.source_tools.is_empty()
                                        || mill.source_tools.contains(&slot.tool)
                                })
                                .count(),
                    ),
                    pcb_cam_core::LayerGeometry::Gerber(_) => None,
                })
                .unwrap_or(0);
            format!(
                "Hole mill · {} · {objects} objects · {:.2} mm{enabled}",
                tool_label(app, task),
                mill.depth_mm
            )
        }
        pcb_cam_core::OperationKind::SpotDrill(spot) => {
            let holes = app
                .project()
                .layer(spot.layer)
                .and_then(|layer| match layer.geometry() {
                    pcb_cam_core::LayerGeometry::Excellon(excellon) => Some(
                        excellon
                            .holes
                            .iter()
                            .filter(|hole| {
                                spot.source_tools.is_empty()
                                    || spot.source_tools.contains(&hole.tool)
                            })
                            .count(),
                    ),
                    pcb_cam_core::LayerGeometry::Gerber(_) => None,
                })
                .unwrap_or(0);
            format!(
                "Spot · {} · {holes} holes · {:.2} mm{enabled}",
                tool_label(app, task),
                spot.depth_mm
            )
        }
        pcb_cam_core::OperationKind::Cutout(cutout) => format!(
            "Edge cut · {} · {} · {:.2} mm{enabled}",
            tool_label(app, task),
            cutout.side.label(),
            cutout.depth_mm
        ),
        pcb_cam_core::OperationKind::Engrave(engrave) => format!(
            "Clearance · {} · {}p · {} {:.2} · {:.2} mm{enabled}",
            tool_label(app, task),
            engrave.passes,
            engrave.side.label(),
            engrave.offset_mm,
            engrave.depth_mm
        ),
    }
}
