//! Project toolbar beneath the menu bar.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use pbc_cam_core::Unit;

use crate::app::PbcCam;
use crate::theme;
use crate::ui::widgets;

/// Draw the workspace toolbar.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::PANEL)
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, theme::BORDER));

    egui::Panel::top("toolbar")
        .exact_size(50.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                widgets::brand_mark(ui, 27.0);
                ui.label(
                    RichText::new("PBC-CAM")
                        .size(14.5)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!("v{}", crate::app::VERSION))
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                );

                ui.add_space(2.0);
                ui.separator();

                ui.label(
                    RichText::new("PROJECT")
                        .size(9.5)
                        .strong()
                        .color(theme::TEXT_FAINT),
                );
                ui.label(
                    RichText::new(app.project().name())
                        .size(13.0)
                        .strong()
                        .color(theme::TEXT),
                );
                if app.session.dirty() {
                    ui.label(RichText::new("*").size(12.0).color(theme::COPPER));
                }
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {}",
                        layer_count_label(app.project().layers().len()),
                        task_count_label(app.project().tasks().len())
                    ))
                    .size(10.5)
                    .color(theme::TEXT_MUTED),
                );

                ui.add_space(4.0);
                ui.separator();

                let gerber = ui
                    .add(Button::new(RichText::new("Gerber").size(11.5)))
                    .on_hover_text("Import a Gerber layer (Cmd/Ctrl+O)");
                if gerber.clicked() {
                    app.import_gerber_dialog();
                }
                let excellon = ui
                    .add(Button::new(RichText::new("Excellon").size(11.5)))
                    .on_hover_text("Import an Excellon drill file (Cmd/Ctrl+Shift+O)");
                if excellon.clicked() {
                    app.import_excellon_dialog();
                }

                ui.menu_button(RichText::new("+ Task").size(11.5), |ui| {
                    let drill = ui.add_enabled(app.can_add_drill_task(), Button::new("Drilling"));
                    if drill.clicked() {
                        app.add_drill_task();
                        ui.close();
                    }
                    let cutout =
                        ui.add_enabled(app.can_add_cutout_task(), Button::new("Contour cut"));
                    if cutout.clicked() {
                        app.add_cutout_task();
                        ui.close();
                    }
                    let engrave =
                        ui.add_enabled(app.can_add_engrave_task(), Button::new("Engraving"));
                    if engrave.clicked() {
                        app.add_engrave_task();
                        ui.close();
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    unit_switch(ui, app);

                    ui.add_space(2.0);
                    ui.separator();

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
        .fill(theme::CANVAS)
        .corner_radius(5)
        .inner_margin(Margin::same(2))
        .stroke(Stroke::new(1.0, theme::BORDER))
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
