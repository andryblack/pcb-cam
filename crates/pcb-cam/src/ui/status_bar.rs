//! Bottom status bar.

use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::PbcCam;
use crate::theme;
use crate::view;

/// Draw the status bar at the bottom of the window.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let frame = Frame::new()
        .fill(theme::app_bg())
        .inner_margin(Margin::symmetric(12, 3))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::bottom("status_bar")
        .exact_size(28.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                ui.painter().circle_filled(
                    dot_rect.center(),
                    3.0,
                    if app.status_error {
                        theme::error()
                    } else {
                        theme::accent()
                    },
                );

                ui.label(
                    RichText::new(&app.status)
                        .size(10.5)
                        .color(if app.status_error {
                            theme::error()
                        } else {
                            theme::text_muted()
                        }),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "{} · {} objects{}",
                        layer_count(app.project().layers().len()),
                        app.project().object_count(),
                        if app.session.dirty() {
                            " · unsaved"
                        } else {
                            ""
                        }
                    ))
                    .size(10.0)
                    .color(theme::text_faint()),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("Zoom {:.0}%", app.view.zoom_percent()))
                            .size(10.0)
                            .monospace()
                            .color(theme::text_muted()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(app.project().units().symbol().to_ascii_uppercase())
                            .size(10.0)
                            .monospace()
                            .color(theme::copper()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(cursor_position(app))
                            .size(10.0)
                            .monospace()
                            .color(theme::text_muted()),
                    );
                });
            });
        });
}

fn cursor_position(app: &PbcCam) -> String {
    let units = app.project().units();
    match app.view.hovered_mm() {
        Some(position) => format!(
            "X {}   Y {}",
            view::format_coordinate(position.x, units),
            view::format_coordinate(position.y, units)
        ),
        None => "X   -        Y   -".to_owned(),
    }
}

fn layer_count(count: usize) -> String {
    if count == 1 {
        "1 layer".to_owned()
    } else {
        format!("{count} layers")
    }
}
