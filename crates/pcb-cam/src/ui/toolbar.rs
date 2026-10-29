//! Project toolbar beneath the menu bar.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use pcb_cam_core::Unit;

use crate::app::PbcCam;
use crate::theme;
use crate::ui::widgets;

/// Draw the workspace toolbar.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::top("toolbar")
        .exact_size(50.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                widgets::brand_mark(ui, 27.0);
                ui.label(
                    RichText::new("PCB-CAM")
                        .size(14.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("v{}", crate::app::VERSION))
                        .size(9.5)
                        .color(theme::text_faint()),
                );

                ui.add_space(2.0);
                ui.separator();

                ui.label(
                    RichText::new("PROJECT")
                        .size(9.5)
                        .strong()
                        .color(theme::text_faint()),
                );
                ui.label(
                    RichText::new(app.project().name())
                        .size(13.0)
                        .strong()
                        .color(theme::text()),
                );
                if app.session.dirty() {
                    ui.label(RichText::new("*").size(12.0).color(theme::copper()));
                }
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {}",
                        layer_count_label(app.project().layers().len()),
                        task_count_label(app.project().tasks().len())
                    ))
                    .size(10.5)
                    .color(theme::text_muted()),
                );

                ui.add_space(4.0);
                ui.separator();

                let import = ui
                    .add(Button::new(RichText::new("Import").size(11.5)))
                    .on_hover_text("Import Gerber, Excellon or a ZIP archive");
                if import.clicked() {
                    app.import_dialog();
                }

                ui.menu_button(RichText::new("+ Task").size(11.5), |ui| {
                    let drill = ui.add_enabled(app.can_add_drill_task(), Button::new("Drilling"));
                    if drill.clicked() {
                        app.add_drill_task();
                        ui.close();
                    }
                    let hole_mill =
                        ui.add_enabled(app.can_add_hole_mill_task(), Button::new("Hole milling"));
                    if hole_mill.clicked() {
                        app.add_hole_mill_task();
                        ui.close();
                    }
                    let spot =
                        ui.add_enabled(app.can_add_spot_drill_task(), Button::new("Spot drill"));
                    if spot.clicked() {
                        app.add_spot_drill_task();
                        ui.close();
                    }
                    let cutout = ui.add_enabled(app.can_add_cutout_task(), Button::new("Edge cut"));
                    if cutout.clicked() {
                        app.add_cutout_task();
                        ui.close();
                    }
                    let engrave =
                        ui.add_enabled(app.can_add_engrave_task(), Button::new("Clearance"));
                    if engrave.clicked() {
                        app.add_engrave_task();
                        ui.close();
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    unit_switch(ui, app);

                    ui.add_space(2.0);
                    ui.separator();

                    let zero = ui
                        .add(
                            Button::new(RichText::new("0-point").size(11.5))
                                .selected(app.origin_pick || app.origin_dialog.is_some()),
                        )
                        .on_hover_text(
                            "Set the G-code X0 Y0 point (double-click the G0 marker for values)",
                        );
                    if zero.clicked() {
                        app.open_origin_dialog();
                    }

                    let grid = ui
                        .add(
                            Button::new(RichText::new("Grid").size(11.5))
                                .selected(app.settings.show_grid),
                        )
                        .on_hover_text("Show grid (G)");
                    if grid.clicked() {
                        app.settings.show_grid = !app.settings.show_grid;
                    }

                    let fit = ui
                        .add(Button::new(RichText::new("Fit").size(11.5)))
                        .on_hover_text("Fit project into the canvas (F)");
                    if fit.clicked() {
                        app.fit_view();
                    }

                    ui.separator();
                    let redo = ui
                        .add_enabled(
                            app.session.can_redo(),
                            Button::new(RichText::new("Redo").size(11.5)),
                        )
                        .on_hover_text("Redo");
                    if redo.clicked() {
                        app.redo();
                    }
                    let undo = ui
                        .add_enabled(
                            app.session.can_undo(),
                            Button::new(RichText::new("Undo").size(11.5)),
                        )
                        .on_hover_text("Undo");
                    if undo.clicked() {
                        app.undo();
                    }
                });
            });
        });
}

fn unit_switch(ui: &mut Ui, app: &mut PbcCam) {
    let current = app.project().units();
    let mut picked = current;

    Frame::new()
        .fill(theme::canvas())
        .corner_radius(5)
        .inner_margin(Margin::same(2))
        .stroke(Stroke::new(1.0, theme::border()))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        vec2(28.0, 20.0),
                        Button::selectable(picked == Unit::Inch, "in"),
                    )
                    .clicked()
                {
                    picked = Unit::Inch;
                }
                if ui
                    .add_sized(
                        vec2(28.0, 20.0),
                        Button::selectable(picked == Unit::Millimeter, "mm"),
                    )
                    .clicked()
                {
                    picked = Unit::Millimeter;
                }
            });
        });

    if picked != current {
        app.set_units(picked);
    }
}

fn layer_count_label(count: usize) -> String {
    if count == 1 {
        "1 layer".to_owned()
    } else {
        format!("{count} layers")
    }
}

fn task_count_label(count: usize) -> String {
    if count == 1 {
        "1 task".to_owned()
    } else {
        format!("{count} tasks")
    }
}
