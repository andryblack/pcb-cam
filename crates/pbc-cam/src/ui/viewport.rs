//! Interactive 2D board canvas.

use crate::app::PbcCam;
use crate::theme;
use crate::ui::widgets;
use crate::view::{self, ViewState};
use eframe::egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Painter, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

/// Draw the central canvas and handle its camera controls.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let available = ui.available_size();
    let (response, _) = ui.allocate_painter(available, Sense::click_and_drag());
    let rect = response.rect;
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, theme::CANVAS);

    handle_input(ui, app, &response, rect);

    if app.settings.show_grid {
        draw_grid(&painter, rect, &app.view);
    }
    if app.settings.show_origin {
        draw_axes(&painter, rect, &app.view);
    }

    let empty_card = if app.project.is_empty() {
        Some(draw_empty_state(&painter, rect))
    } else {
        None
    };

    draw_overlays(&painter, rect, app);

    if let Some(pointer) = response.hover_pos()
        && !empty_card.is_some_and(|card| card.contains(pointer))
    {
        draw_cursor(&painter, pointer);
    }
}

fn handle_input(ui: &Ui, app: &mut PbcCam, response: &Response, rect: Rect) {
    if response.dragged() {
        app.view.pan_by(response.drag_delta());
    }

    if response.double_clicked() {
        app.reset_view();
    }

    if response.hovered()
        && let Some(pointer) = response.hover_pos()
    {
        let (scroll, zoom_delta, modifiers) = ui.input(|input| {
            (
                input.smooth_scroll_delta.y,
                input.zoom_delta(),
                input.modifiers,
            )
        });

        if (zoom_delta - 1.0).abs() > 0.001 {
            app.view.zoom_at(zoom_delta, pointer, rect);
        } else if scroll.abs() > 0.01 && !modifiers.ctrl && !modifiers.command {
            let factor = (scroll * 0.0025).exp();
            app.view.zoom_at(factor, pointer, rect);
        }
    }

    if response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    }

    app.view.set_hovered_mm(
        response
            .hover_pos()
            .map(|pointer| app.view.screen_to_world(pointer, rect)),
    );
}

fn draw_grid(painter: &Painter, rect: Rect, view: &ViewState) {
    let step = view::grid_step_mm(view.zoom());
    let top_left = view.screen_to_world(rect.left_top(), rect);
    let bottom_right = view.screen_to_world(rect.right_bottom(), rect);

    let x_first = (f64::from(top_left.x) / step).floor() as i64;
    let x_last = (f64::from(bottom_right.x) / step).ceil() as i64;
    let y_first = (f64::from(bottom_right.y) / step).floor() as i64;
    let y_last = (f64::from(top_left.y) / step).ceil() as i64;

    const MAX_LINES_PER_AXIS: i64 = 4_000;
    if (x_last - x_first).abs() > MAX_LINES_PER_AXIS
        || (y_last - y_first).abs() > MAX_LINES_PER_AXIS
    {
        return;
    }

    for index in x_first..=x_last {
        let x = (index as f64 * step) as f32;
        let screen_x = view.world_to_screen(pos2(x, 0.0), rect).x;
        let major = index.rem_euclid(10) == 0;
        painter.line_segment(
            [pos2(screen_x, rect.top()), pos2(screen_x, rect.bottom())],
            Stroke::new(
                1.0,
                if major {
                    theme::GRID_MAJOR
                } else {
                    theme::GRID_MINOR
                },
            ),
        );
    }

    for index in y_first..=y_last {
        let y = (index as f64 * step) as f32;
        let screen_y = view.world_to_screen(pos2(0.0, y), rect).y;
        let major = index.rem_euclid(10) == 0;
        painter.line_segment(
            [pos2(rect.left(), screen_y), pos2(rect.right(), screen_y)],
            Stroke::new(
                1.0,
                if major {
                    theme::GRID_MAJOR
                } else {
                    theme::GRID_MINOR
                },
            ),
        );
    }
}

fn draw_axes(painter: &Painter, rect: Rect, view: &ViewState) {
    let origin = view.world_to_screen(Pos2::ZERO, rect);
    if !rect.contains(origin) {
        return;
    }

    painter.line_segment(
        [pos2(rect.left(), origin.y), pos2(rect.right(), origin.y)],
        Stroke::new(1.0, theme::AXIS_X),
    );
    painter.line_segment(
        [pos2(origin.x, rect.top()), pos2(origin.x, rect.bottom())],
        Stroke::new(1.0, theme::AXIS_Y),
    );

    painter.text(
        pos2(rect.right() - 10.0, origin.y - 6.0),
        Align2::RIGHT_BOTTOM,
        "X",
        FontId::monospace(10.0),
        theme::AXIS_X,
    );
    painter.text(
        pos2(origin.x + 7.0, rect.top() + 10.0),
        Align2::LEFT_TOP,
        "Y",
        FontId::monospace(10.0),
        theme::AXIS_Y,
    );

    painter.circle_filled(origin, 4.5, theme::CANVAS);
    painter.circle_stroke(origin, 4.5, Stroke::new(1.2, theme::COPPER));
    painter.circle_filled(origin, 1.6, theme::COPPER_BRIGHT);
}

fn draw_empty_state(painter: &Painter, rect: Rect) -> Rect {
    let width = (rect.width() - 48.0).clamp(280.0, 460.0);
    let height = 216.0;
    let card = Rect::from_center_size(rect.center() + vec2(0.0, -4.0), vec2(width, height));

    painter.rect_filled(
        card,
        CornerRadius::same(12),
        Color32::from_rgba_unmultiplied(17, 22, 28, 248),
    );
    painter.rect_stroke(
        card,
        CornerRadius::same(12),
        Stroke::new(1.0, theme::BORDER_STRONG),
        StrokeKind::Inside,
    );
    painter.line_segment(
        [
            card.center_top() + vec2(-26.0, 0.0),
            card.center_top() + vec2(26.0, 0.0),
        ],
        Stroke::new(2.0, theme::COPPER),
    );

    let logo = Rect::from_center_size(pos2(card.center().x, card.top() + 48.0), Vec2::splat(46.0));
    widgets::paint_mark(painter, logo);

    painter.text(
        pos2(card.center().x, card.top() + 98.0),
        Align2::CENTER_CENTER,
        "No layers loaded",
        FontId::proportional(19.0),
        theme::TEXT,
    );
    painter.text(
        pos2(card.center().x, card.top() + 125.0),
        Align2::CENTER_CENTER,
        "Gerber and Excellon import is coming next.",
        FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
    painter.text(
        pos2(card.center().x, card.top() + 144.0),
        Align2::CENTER_CENTER,
        "The canvas and project model are ready.",
        FontId::proportional(12.0),
        theme::TEXT_FAINT,
    );

    let footer_line_y = card.bottom() - 35.0;
    painter.line_segment(
        [
            pos2(card.left() + 24.0, footer_line_y),
            pos2(card.right() - 24.0, footer_line_y),
        ],
        Stroke::new(1.0, theme::BORDER),
    );
    painter.text(
        pos2(card.center().x, card.bottom() - 18.0),
        Align2::CENTER_CENTER,
        "PBC-CAM  /  PCB FABRICATION WORKBENCH",
        FontId::proportional(9.0),
        theme::TEXT_FAINT,
    );

    card
}

fn draw_overlays(painter: &Painter, rect: Rect, app: &PbcCam) {
    let mut x = rect.left() + 14.0;
    let y = rect.top() + 14.0;

    let chip_rect = chip(painter, pos2(x, y), "TOP / XY");
    x = chip_rect.right() + 8.0;

    if app.settings.show_grid {
        let grid_label = format!(
            "GRID {}",
            view::format_length(
                view::grid_step_mm(app.view.zoom()) as f32,
                app.project.units(),
            )
        );
        chip(painter, pos2(x, y), &grid_label);
    }

    draw_scale_bar(painter, rect, app);
}

fn chip(painter: &Painter, anchor: Pos2, text: &str) -> Rect {
    let galley =
        painter.layout_no_wrap(text.to_owned(), FontId::monospace(10.0), theme::TEXT_MUTED);
    let rect = Rect::from_min_size(anchor, galley.size() + vec2(16.0, 8.0));

    painter.rect_filled(
        rect,
        CornerRadius::same(5),
        Color32::from_rgba_unmultiplied(17, 22, 28, 236),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(8.0, 4.0), galley, theme::TEXT_MUTED);
    rect
}

fn draw_scale_bar(painter: &Painter, rect: Rect, app: &PbcCam) {
    let zoom = app.view.zoom();
    let length_mm = view::scale_bar_mm(zoom, 120.0);
    let length_px = (length_mm as f32 * zoom).clamp(20.0, rect.width() * 0.45);
    let start = pos2(rect.left() + 20.0, rect.bottom() - 24.0);
    let end = pos2(start.x + length_px, start.y);

    let line = Stroke::new(1.4, theme::TEXT_MUTED);
    painter.line_segment([start, end], line);
    painter.line_segment([start, pos2(start.x, start.y - 5.0)], line);
    painter.line_segment([end, pos2(end.x, end.y - 5.0)], line);

    painter.text(
        pos2(start.x + length_px / 2.0, start.y - 8.0),
        Align2::CENTER_BOTTOM,
        view::format_length(length_mm as f32, app.project.units()),
        FontId::monospace(10.0),
        theme::TEXT_MUTED,
    );
}

fn draw_cursor(painter: &Painter, pointer: Pos2) {
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(227, 164, 94, 165));
    painter.line_segment([pointer - vec2(8.0, 0.0), pointer - vec2(3.0, 0.0)], stroke);
    painter.line_segment([pointer + vec2(3.0, 0.0), pointer + vec2(8.0, 0.0)], stroke);
    painter.line_segment([pointer - vec2(0.0, 8.0), pointer - vec2(0.0, 3.0)], stroke);
    painter.line_segment([pointer + vec2(0.0, 3.0), pointer + vec2(0.0, 8.0)], stroke);
}
