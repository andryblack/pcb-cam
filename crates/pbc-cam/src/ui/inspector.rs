//! Right-hand inspector panel.

use eframe::egui::{self, Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use pbc_cam_core::Unit;

use crate::app::PbcCam;
use crate::theme;
use crate::ui::widgets;
use crate::view;

/// Draw the right inspector panel.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::PANEL)
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, theme::BORDER));

    egui::Panel::right("inspector")
        .default_size(268.0)
        .min_size(230.0)
        .max_size(380.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            widgets::panel_header(ui, "Inspector", &mut app.settings.show_inspector);
            ui.add_space(8.0);

            layer_details(ui, app);

            ui.add_space(18.0);
            widgets::hairline(ui);
            ui.add_space(10.0);
            widgets::section_label(ui, "View");
            ui.add_space(6.0);

            ui.checkbox(&mut app.settings.show_grid, "Show grid");
            ui.checkbox(&mut app.settings.show_origin, "Show origin and axes");

            ui.add_space(8.0);
            unit_switch(ui, app);

            ui.add_space(12.0);
            field_value(ui, "Zoom", &format!("{:.0} %", app.view.zoom_percent()));
            field_value(
                ui,
                "Grid",
                &view::format_length(
                    view::grid_step_mm(app.view.zoom()) as f32,
                    app.project.units(),
                ),
            );
            field_value(ui, "View", "Top / XY");

            ui.add_space(10.0);
            let reset = ui.add_sized(
                vec2(ui.available_width(), 26.0),
                Button::new(RichText::new("Reset view").size(11.5)),
            );
            if reset.clicked() {
                app.reset_view();
            }

            ui.add_space(18.0);
            widgets::hairline(ui);
            ui.add_space(10.0);
            widgets::section_label(ui, "Canvas");
            ui.add_space(5.0);
            ui.label(
                RichText::new("Scroll to zoom around the pointer. Drag to pan.")
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
        });
}

fn layer_details(ui: &mut Ui, app: &mut PbcCam) {
    let Some(id) = app.selected_layer else {
        widgets::hint_card(
            ui,
            "No layer selected",
            "Select a layer in the project tree to inspect its source and properties.",
        );
        return;
    };

    let Some(layer) = app.project.layer(id) else {
        app.selected_layer = None;
        return;
    };

    let name = layer.name().to_owned();
    let format = layer.format().label();
    let source = layer
        .source()
        .and_then(|path| path.file_name())
        .map_or_else(
            || "Not read from disk".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );

    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(name).size(13.0).strong().color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::pill(ui, format, theme::ACCENT);
            });
        });
        ui.add_space(8.0);
        widgets::field_row(ui, "Format", format);
        widgets::field_row(ui, "Source", &source);
        widgets::field_row(ui, "Layer ID", &id.value().to_string());
    });
}

fn unit_switch(ui: &mut Ui, app: &mut PbcCam) {
    let current = app.project.units();
    let mut picked = current;

    ui.horizontal(|ui| {
        widgets::section_label(ui, "Units");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(Button::selectable(picked == Unit::Inch, "in"))
                .clicked()
            {
                picked = Unit::Inch;
            }
            if ui
                .add(Button::selectable(picked == Unit::Millimeter, "mm"))
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

fn field_value(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(theme::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .size(11.0)
                    .monospace()
                    .color(theme::TEXT),
            );
        });
    });
}
