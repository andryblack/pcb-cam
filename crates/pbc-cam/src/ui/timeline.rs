//! Operation timeline at the bottom of the workspace.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};

use crate::app::{PbcCam, Selection};
use crate::theme;
use crate::ui::widgets;

/// Draw the operation timeline above the status bar.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::PANEL)
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, theme::BORDER));

    egui::Panel::bottom("operation_timeline")
        .exact_size(84.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_label(ui, "Operations");
                widgets::pill(
                    ui,
                    &app.project().operations().len().to_string(),
                    theme::TEXT_MUTED,
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Ordered drilling and routing steps for the NC program",
                    )
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can_export = app.project().enabled_drill_count() > 0;
                    let export = ui.add_enabled(
                        can_export,
                        Button::new(RichText::new("Export G-code").size(11.0)),
                    );
                    if export.clicked() {
                        app.export_gcode_dialog();
                    }
                    if !can_export {
                        export.on_disabled_hover_text(
                            "Add and enable a drilling operation first",
                        );
                    }

                    let can_add = app.can_add_drill_operation();
                    let add = ui.add_enabled(
                        can_add,
                        Button::new(RichText::new("+ Drill").size(11.0)),
                    );
                    if add.clicked() {
                        app.add_drill_operation();
                    }
                    if !can_add {
                        add.on_disabled_hover_text(
                            "Import an Excellon layer with holes or slots first",
                        );
                    }
                });
            });

            ui.add_space(5.0);

            if app.project().operations().is_empty() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            "No operations yet — import Excellon drills, then add a drilling operation.",
                        )
                        .size(11.0)
                        .color(theme::TEXT_FAINT),
                    );
                });
                return;
            }

            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut selected_change = None;
                        for (index, operation) in
                            app.project().operations().iter().enumerate()
                        {
                            let selected =
                                app.selection == Some(Selection::Operation(operation.id()));
                            let color = if operation.enabled() {
                                theme::COPPER
                            } else {
                                theme::TEXT_FAINT
                            };
                            Frame::new()
                                .fill(if selected {
                                    theme::SURFACE_ACTIVE
                                } else {
                                    theme::CANVAS
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected {
                                        theme::COPPER_DIM
                                    } else {
                                        theme::BORDER
                                    },
                                ))
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(9, 5))
                                .show(ui, |ui| {
                                    ui.set_min_width(168.0);
                                    ui.horizontal(|ui| {
                                        let (dot, _) = ui.allocate_exact_size(
                                            vec2(7.0, 7.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().circle_filled(dot.center(), 3.0, color);
                                        let title = RichText::new(format!(
                                            "{}. {}",
                                            index + 1,
                                            operation.name()
                                        ))
                                        .size(11.5)
                                        .strong()
                                        .color(if selected {
                                            theme::COPPER_BRIGHT
                                        } else {
                                            theme::TEXT
                                        });
                                        if ui
                                            .add(
                                                Button::new(title)
                                                    .frame(false),
                                            )
                                            .clicked()
                                        {
                                            selected_change =
                                                Some(Selection::Operation(operation.id()));
                                        }
                                    });
                                    ui.label(
                                        RichText::new(operation_subtitle(operation))
                                            .size(9.5)
                                            .color(theme::TEXT_MUTED),
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

fn operation_subtitle(operation: &pbc_cam_core::Operation) -> String {
    match operation.kind() {
        pbc_cam_core::OperationKind::Drill(drill) => format!(
            "Drill · depth {:.2} mm · {}",
            drill.depth_mm,
            if operation.enabled() {
                "enabled"
            } else {
                "disabled"
            }
        ),
    }
}
