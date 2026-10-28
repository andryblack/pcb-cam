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
                widgets::pill(ui, "UI PREVIEW", theme::COPPER);

                ui.add_space(4.0);
                ui.separator();

                ui.label(
                    RichText::new("PROJECT")
                        .size(9.5)
                        .strong()
                        .color(theme::TEXT_FAINT),
                );
                ui.label(
                    RichText::new(app.project.name())
                        .size(13.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(layer_count_label(app.project.layers().len()))
                        .size(10.5)
                        .color(theme::TEXT_MUTED),
                );

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
                        .add(Button::new(RichText::new("Fit view").size(11.5)))
                        .on_hover_text("Reset canvas view (F)");
                    if fit.clicked() {
                        app.reset_view();
                    }
                });
            });
        });
}

fn unit_switch(ui: &mut Ui, app: &mut PbcCam) {
    let current = app.project.units();
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
