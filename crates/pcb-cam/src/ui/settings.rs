//! Application settings window.

use eframe::egui::{self, Align, Button, Frame, Layout, RichText, Ui, vec2};
use pcb_cam_core::MachiningDefaults;

use crate::app::{PbcCam, SettingsSection};
use crate::theme::{self, ThemeMode};
use crate::ui::widgets;

pub(crate) fn show(ctx: &egui::Context, app: &mut PbcCam) {
    if !app.settings_open {
        return;
    }
    let mut open = true;
    let mut section = app.settings_section;
    let mut appearance = app.appearance;
    let original_appearance = appearance;
    let mut machining = *app.session.tool_library().machining();
    let original_machining = machining;
    let tools = app.session.tool_library().tools().to_vec();

    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .default_width(640.0)
        .frame(crate::app::dialog_frame())
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                Frame::new()
                    .fill(theme::panel())
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(132.0);
                        ui.vertical(|ui| {
                            if ui
                                .add_sized(
                                    vec2(ui.available_width(), 26.0),
                                    Button::selectable(
                                        section == SettingsSection::Appearance,
                                        "Appearance",
                                    ),
                                )
                                .clicked()
                            {
                                section = SettingsSection::Appearance;
                            }
                            if ui
                                .add_sized(
                                    vec2(ui.available_width(), 26.0),
                                    Button::selectable(
                                        section == SettingsSection::Machining,
                                        "Machining",
                                    ),
                                )
                                .clicked()
                            {
                                section = SettingsSection::Machining;
                            }
                        });
                    });

                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.set_width(450.0);
                    egui::ScrollArea::vertical()
                        .max_height(560.0)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, |ui| match section {
                            SettingsSection::Appearance => {
                                appearance_ui(ui, &mut appearance);
                            }
                            SettingsSection::Machining => {
                                machining_ui(ui, &mut machining, &tools);
                            }
                        });
                });
            });
        });

    app.settings_open = open;
    app.settings_section = section;
    if machining != original_machining {
        app.session.tool_library_mut().set_machining(machining);
    }
    if appearance != original_appearance {
        app.appearance = appearance;
        theme::apply(ctx, &appearance);
        ctx.request_repaint();
    }
}

fn appearance_ui(ui: &mut Ui, appearance: &mut theme::AppearanceSettings) {
    ui.heading("Appearance");
    ui.label(
        RichText::new("Theme and canvas object colours.")
            .size(11.0)
            .color(theme::text_muted()),
    );
    ui.add_space(12.0);

    widgets::section_label(ui, "Theme");
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        for mode in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System] {
            ui.selectable_value(&mut appearance.theme, mode, mode.label());
        }
    });
    if appearance.theme == ThemeMode::System {
        ui.label(
            RichText::new("Follows the operating-system appearance.")
                .size(10.5)
                .color(theme::text_faint()),
        );
    }

    ui.add_space(14.0);
    widgets::section_label(ui, "Canvas objects");
    ui.add_space(4.0);
    color_row(ui, "Gerber fill", &mut appearance.colors.gerber_fill);
    color_row(ui, "Gerber outline", &mut appearance.colors.gerber_outline);
    color_row(ui, "Excellon", &mut appearance.colors.excellon);
    color_row(ui, "Task overlay", &mut appearance.colors.task_overlay);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button("Reset colours").clicked() {
            appearance.colors = theme::CanvasColors::default();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("Changes apply immediately")
                    .size(10.0)
                    .color(theme::text_faint()),
            );
        });
    });
}

fn machining_ui(ui: &mut Ui, machining: &mut MachiningDefaults, tools: &[pcb_cam_core::Tool]) {
    ui.heading("Machining");
    ui.label(
        RichText::new("Defaults for newly created tasks and travel moves.")
            .size(11.0)
            .color(theme::text_muted()),
    );
    ui.add_space(12.0);

    widgets::section_label(ui, "Travel and safety");
    ui.add_space(3.0);
    number_row(
        ui,
        "Travel feed",
        &mut machining.rapid_feed_mm_min,
        10.0,
        0.0..=30_000.0,
        " mm/min",
        0,
    );
    number_row(
        ui,
        "Safe Z height",
        &mut machining.safe_z_mm,
        0.1,
        0.0..=100.0,
        " mm",
        3,
    );
    number_row(
        ui,
        "Spindle",
        &mut machining.spindle_rpm,
        100.0,
        0.0..=60_000.0,
        " rpm",
        0,
    );

    ui.add_space(12.0);
    widgets::section_label(ui, "Drilling");
    ui.add_space(3.0);
    number_row(
        ui,
        "Plunge feed",
        &mut machining.drill_plunge_mm_min,
        5.0,
        0.1..=5_000.0,
        " mm/min",
        0,
    );
    number_row(
        ui,
        "Depth",
        &mut machining.drill_depth_mm,
        0.1,
        0.01..=50.0,
        " mm",
        3,
    );
    number_row(
        ui,
        "Spot depth",
        &mut machining.spot_depth_mm,
        0.05,
        0.01..=50.0,
        " mm",
        3,
    );

    ui.add_space(12.0);
    widgets::section_label(ui, "Milling");
    ui.add_space(3.0);
    number_row(
        ui,
        "Feed",
        &mut machining.mill_feed_mm_min,
        10.0,
        0.1..=30_000.0,
        " mm/min",
        0,
    );
    number_row(
        ui,
        "Plunge feed",
        &mut machining.mill_plunge_mm_min,
        5.0,
        0.1..=5_000.0,
        " mm/min",
        0,
    );
    number_row(
        ui,
        "Edge-cut depth",
        &mut machining.mill_depth_mm,
        0.1,
        0.01..=50.0,
        " mm",
        3,
    );
    number_row(
        ui,
        "Helical step-down",
        &mut machining.helical_stepdown_mm,
        0.05,
        0.01..=20.0,
        " mm",
        3,
    );

    ui.add_space(12.0);
    widgets::section_label(ui, "Clearance");
    ui.add_space(3.0);
    number_row(
        ui,
        "Depth",
        &mut machining.clearance_depth_mm,
        0.05,
        0.01..=20.0,
        " mm",
        3,
    );
    clearance_tool_row(ui, machining, tools);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button("Reset machining defaults").clicked() {
            *machining = MachiningDefaults::default();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("Applies to new tasks")
                    .size(10.0)
                    .color(theme::text_faint()),
            );
        });
    });
}

fn number_row(
    ui: &mut Ui,
    label: &str,
    value: &mut impl egui::emath::Numeric,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    decimals: usize,
) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(label).size(11.5).color(theme::text()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(value)
                    .speed(speed)
                    .range(range)
                    .fixed_decimals(decimals)
                    .suffix(suffix),
            );
        });
    });
    ui.add_space(2.0);
}

fn clearance_tool_row(
    ui: &mut Ui,
    machining: &mut MachiningDefaults,
    tools: &[pcb_cam_core::Tool],
) {
    let selected = machining
        .clearance_tool
        .and_then(|id| tools.iter().find(|tool| tool.id() == id))
        .map(|tool| format!("{} · Ø{:.2} mm", tool.name(), tool.diameter_mm()))
        .unwrap_or_else(|| "Auto".to_owned());
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new("Default tool")
                .size(11.5)
                .color(theme::text()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            egui::ComboBox::from_id_salt("clearance_default_tool")
                .selected_text(selected)
                .width(190.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut machining.clearance_tool, None, "Auto");
                    for tool in tools.iter().filter(|tool| tool.kind().can_interpolate()) {
                        ui.selectable_value(
                            &mut machining.clearance_tool,
                            Some(tool.id()),
                            format!("{} · Ø{:.2} mm", tool.name(), tool.diameter_mm()),
                        );
                    }
                });
        });
    });
}

fn color_row(ui: &mut Ui, label: &str, color: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(label).size(11.5).color(theme::text()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.color_edit_button_srgb(color);
        });
    });
    ui.add_space(2.0);
}
