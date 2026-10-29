//! Small shared widgets and painting helpers.

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, Frame, Layout, Margin, Painter, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::theme;

/// Paint the PCB-CAM mark into `rect`.
pub(crate) fn paint_mark(painter: &Painter, rect: Rect) {
    let size = rect.width().min(rect.height());
    let scale = size / 64.0;
    let point = |x: f32, y: f32| rect.min + vec2(x * scale, y * scale);

    let radius = (12.0 * scale).round().clamp(2.0, 255.0) as u8;
    painter.rect_filled(rect, CornerRadius::same(radius), theme::app_bg());
    painter.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new((1.0 * scale).max(0.5), theme::copper_dim()),
        StrokeKind::Inside,
    );

    let trace = Stroke::new((2.2 * scale).max(0.8), theme::copper());
    let glyph = Stroke::new((3.9 * scale).max(1.2), theme::copper_bright());

    // Two copper traces entering the "P".
    painter.line_segment([point(7.0, 22.0), point(21.0, 22.0)], trace);
    painter.line_segment([point(43.0, 42.0), point(57.0, 42.0)], trace);

    // Stylised "P" routed like a PCB trace.
    painter.line_segment([point(22.0, 51.0), point(22.0, 14.0)], glyph);
    painter.line_segment([point(22.0, 14.0), point(38.0, 14.0)], glyph);
    painter.line_segment([point(38.0, 14.0), point(45.0, 20.0)], glyph);
    painter.line_segment([point(45.0, 20.0), point(45.0, 29.0)], glyph);
    painter.line_segment([point(45.0, 29.0), point(38.0, 35.0)], glyph);
    painter.line_segment([point(38.0, 35.0), point(22.0, 35.0)], glyph);

    // Through-hole pads.
    for (x, y) in [(7.0, 22.0), (57.0, 42.0)] {
        painter.circle_filled(point(x, y), 3.4 * scale, theme::app_bg());
        painter.circle_stroke(
            point(x, y),
            3.4 * scale,
            Stroke::new((1.4 * scale).max(0.7), theme::copper()),
        );
    }
}

/// A small painted logo that participates in the UI layout.
pub(crate) fn brand_mark(ui: &mut Ui, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_mark(ui.painter(), rect);
    response
}

/// Uppercase section label used in panels and menus.
pub(crate) fn section_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_ascii_uppercase())
            .size(9.5)
            .strong()
            .color(theme::text_muted()),
    );
}

/// Header row with a title and a button that hides the panel.
pub(crate) fn panel_header(ui: &mut Ui, title: &str, open: &mut bool) {
    ui.horizontal(|ui| {
        section_label(ui, title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let button = ui
                .add(
                    Button::new(RichText::new("×").size(14.0).color(theme::text_muted()))
                        .frame(false),
                )
                .on_hover_text("Hide panel");
            if button.clicked() {
                *open = false;
            }
        });
    });
}

/// Small tinted badge.
pub(crate) fn pill(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            26,
        ))
        .corner_radius(4)
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(9.5).strong().color(color));
        });
}

/// Raised card used for project summaries and dialogs.
pub(crate) fn card_frame() -> Frame {
    Frame::new()
        .fill(theme::panel_raised())
        .corner_radius(8)
        .inner_margin(Margin::same(10))
        .stroke(Stroke::new(1.0, theme::border()))
}

/// A quiet card used for empty states.
pub(crate) fn hint_card(ui: &mut Ui, title: &str, body: &str) {
    Frame::new()
        .fill(theme::canvas())
        .corner_radius(6)
        .inner_margin(Margin::same(10))
        .stroke(Stroke::new(1.0, theme::border()))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(12.0).color(theme::text_muted()));
            ui.add_space(2.0);
            ui.label(RichText::new(body).size(11.0).color(theme::text_faint()));
        });
}

/// Read-only label/value row.
pub(crate) fn field_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(theme::text_muted()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(value)
                        .size(11.0)
                        .monospace()
                        .color(theme::text()),
                )
                .truncate(),
            );
        });
    });
}

/// Draw a subtle horizontal rule spanning the available width.
pub(crate) fn hairline(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().line_segment(
        [pos2(rect.left(), rect.center().y), rect.right_center()],
        Stroke::new(1.0, theme::border()),
    );
}
