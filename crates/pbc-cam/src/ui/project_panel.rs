//! Project and layer tree panel.

use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::PbcCam;
use crate::theme;
use crate::ui::widgets;

/// Draw the left project panel.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::PANEL)
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, theme::BORDER));

    egui::Panel::left("project_panel")
        .default_size(252.0)
        .min_size(220.0)
        .max_size(360.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            widgets::panel_header(ui, "Project", &mut app.settings.show_project_panel);
            ui.add_space(8.0);

            project_card(ui, app);
            ui.add_space(16.0);

            ui.horizontal(|ui| {
                widgets::section_label(ui, "Layers");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::pill(
                        ui,
                        &app.project.layers().len().to_string(),
                        theme::TEXT_MUTED,
                    );
                });
            });
            ui.add_space(5.0);

            if app.project.is_empty() {
                widgets::hint_card(
                    ui,
                    "No source layers",
                    "Gerber and Excellon files will appear here once import lands in the next milestone.",
                );
            } else {
                layer_list(ui, app);
            }

            ui.add_space(18.0);
            widgets::hairline(ui);
            ui.add_space(10.0);
            widgets::section_label(ui, "Output");
            ui.add_space(5.0);
            widgets::hint_card(
                ui,
                "No G-code yet",
                "Toolpaths and post-processed NC output will be generated from visible layers.",
            );
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
                    RichText::new(app.project.name())
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new("PCB fabrication project")
                        .size(10.5)
                        .color(theme::TEXT_MUTED),
                );
            });
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            stat(ui, "LAYERS", &app.project.layers().len().to_string());
            ui.add_space(22.0);
            stat(
                ui,
                "UNITS",
                &app.project.units().symbol().to_ascii_uppercase(),
            );
            ui.add_space(22.0);
            stat(ui, "TOOLPATHS", "0");
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

fn layer_list(ui: &mut Ui, app: &mut PbcCam) {
    let mut visibility_change = None;
    let mut selection_change = None;

    for layer in app.project.layers() {
        let selected = app.selected_layer == Some(layer.id());
        let mut visible = layer.visible();

        ui.horizontal(|ui| {
            if ui
                .add(egui::Checkbox::new(&mut visible, ""))
                .on_hover_text("Toggle layer visibility")
                .changed()
            {
                visibility_change = Some((layer.id(), visible));
            }

            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
            ui.painter().circle_filled(
                dot_rect.center(),
                3.0,
                if visible {
                    theme::COPPER
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
                selection_change = Some(layer.id());
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(layer.format().label())
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                );
            });
        });
    }

    if let Some((id, visible)) = visibility_change {
        app.project.set_layer_visible(id, visible);
    }
    if let Some(id) = selection_change {
        app.selected_layer = Some(id);
        app.status = format!(
            "Selected layer: {}",
            app.project
                .layer(id)
                .map_or("unknown", pbc_cam_core::Layer::name)
        );
    }
}
