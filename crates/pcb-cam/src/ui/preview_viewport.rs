//! 3D toolpath viewport for the G-code preview.

use eframe::egui::{
    Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Frame, Margin, Painter,
    PointerButton, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use pcb_cam_core::preview::{GcodePreview, Point3, PreviewState};

use crate::preview::{CameraPreset, PreviewSession, Vec3, scroll_zoom};
use crate::theme;
use crate::view;

/// Draw the central 3D toolpath view.
pub(crate) fn show(ui: &mut Ui, preview: &mut PreviewSession) {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
    let rect = response.rect;
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return;
    }
    painter.rect_filled(rect, CornerRadius::ZERO, theme::canvas());

    if preview.preview.is_none() {
        draw_unavailable(&painter, rect, preview.error.as_deref());
        return;
    }

    handle_input(ui, preview, &response);
    if preview.fit_pending
        && let Some(data) = preview.preview.as_ref()
    {
        let mut max = data.max;
        max.z += 4.0;
        preview.camera.fit(data.min, max, preview.z_scale, rect);
        preview.fit_pending = false;
    }

    let Some(data) = preview.preview.as_ref() else {
        return;
    };
    draw_scene(&painter, rect, preview, data);
    draw_overlay(&painter, rect, preview, data);
    if rect.width() >= 560.0 {
        draw_view_controls(ui, rect, preview);
    }
}

fn handle_input(ui: &mut Ui, preview: &mut PreviewSession, response: &eframe::egui::Response) {
    let modifiers = ui.input(|input| input.modifiers);
    if response.dragged() {
        let delta = response.drag_delta();
        if modifiers.shift
            || response.dragged_by(PointerButton::Secondary)
            || response.dragged_by(PointerButton::Middle)
        {
            preview.camera.pan_by(delta);
        } else {
            preview.camera.orbit(delta);
        }
    }
    if response.double_clicked() {
        preview.fit_pending = true;
    }
    if response.hovered()
        && let Some(pointer) = response.hover_pos()
    {
        let (scroll, zoom_delta) =
            ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
        if (zoom_delta - 1.0).abs() > 0.001 {
            preview.camera.zoom_at(zoom_delta, pointer, response.rect);
        } else if scroll.abs() > 0.01 {
            preview
                .camera
                .zoom_at(scroll_zoom(scroll), pointer, response.rect);
        }
        ui.ctx().set_cursor_icon(if response.dragged() {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        });
    }
}

struct Projector {
    right: Vec3,
    up: Vec3,
    target: Vec3,
    center: Pos2,
    zoom: f32,
    z_scale: f32,
}

impl Projector {
    fn new(preview: &PreviewSession, rect: Rect) -> Self {
        let (right, up, _) = preview.camera.basis();
        Self {
            right,
            up,
            target: preview.camera.target,
            center: rect.center() + preview.camera.pan,
            zoom: preview.camera.zoom,
            z_scale: preview.z_scale,
        }
    }

    fn point(&self, point: Point3) -> Pos2 {
        let dx = point.x as f32 - self.target.x;
        let dy = point.y as f32 - self.target.y;
        let dz = point.z as f32 * self.z_scale - self.target.z;
        pos2(
            self.center.x + (self.right.x * dx + self.right.y * dy + self.right.z * dz) * self.zoom,
            self.center.y - (self.up.x * dx + self.up.y * dy + self.up.z * dz) * self.zoom,
        )
    }

    fn surface(&self, x: f64, y: f64) -> Pos2 {
        self.point(Point3::new(x, y, 0.0))
    }
}

fn draw_scene(painter: &Painter, rect: Rect, preview: &PreviewSession, data: &GcodePreview) {
    let projector = Projector::new(preview, rect);
    draw_grid(painter, preview, data, &projector);
    draw_axes(painter, &projector);
    draw_paths(painter, preview, data, &projector);
    let state = data.state_at(preview.time_s);
    draw_tool(painter, &projector, &state);
}

fn draw_grid(
    painter: &Painter,
    preview: &PreviewSession,
    data: &GcodePreview,
    projector: &Projector,
) {
    let pad = ((data.max.x - data.min.x).max(data.max.y - data.min.y) * 0.06).max(1.0);
    let min_x = data.min.x - pad;
    let max_x = data.max.x + pad;
    let min_y = data.min.y - pad;
    let max_y = data.max.y + pad;

    let mut step = view::grid_step_mm(preview.camera.zoom);
    while (max_x - min_x) / step > 32.0 || (max_y - min_y) / step > 32.0 {
        step *= 2.0;
    }

    let minor = Color32::from_rgba_unmultiplied(
        theme::grid_minor().r(),
        theme::grid_minor().g(),
        theme::grid_minor().b(),
        190,
    );
    let major = Color32::from_rgba_unmultiplied(
        theme::grid_major().r(),
        theme::grid_major().g(),
        theme::grid_major().b(),
        230,
    );
    let first_x = (min_x / step).floor() as i64;
    let last_x = (max_x / step).ceil() as i64;
    let first_y = (min_y / step).floor() as i64;
    let last_y = (max_y / step).ceil() as i64;

    for index in first_x..=last_x {
        let x = index as f64 * step;
        let color = if index.rem_euclid(10) == 0 {
            major
        } else {
            minor
        };
        painter.line_segment(
            [projector.surface(x, min_y), projector.surface(x, max_y)],
            Stroke::new(1.0, color),
        );
    }
    for index in first_y..=last_y {
        let y = index as f64 * step;
        let color = if index.rem_euclid(10) == 0 {
            major
        } else {
            minor
        };
        painter.line_segment(
            [projector.surface(min_x, y), projector.surface(max_x, y)],
            Stroke::new(1.0, color),
        );
    }

    // Stock outline at Z0.
    let corners = [
        Point3::new(data.min.x, data.min.y, 0.0),
        Point3::new(data.max.x, data.min.y, 0.0),
        Point3::new(data.max.x, data.max.y, 0.0),
        Point3::new(data.min.x, data.max.y, 0.0),
    ];
    let mut outline: Vec<Pos2> = corners
        .iter()
        .map(|point| projector.point(*point))
        .collect();
    outline.push(outline[0]);
    painter.add(eframe::egui::Shape::line(
        outline,
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(
                theme::border_strong().r(),
                theme::border_strong().g(),
                theme::border_strong().b(),
                210,
            ),
        ),
    ));
}

fn draw_axes(painter: &Painter, projector: &Projector) {
    let length = 8.0_f64;
    let origin = projector.point(Point3::new(0.0, 0.0, 0.0));
    let x = projector.point(Point3::new(length, 0.0, 0.0));
    let y = projector.point(Point3::new(0.0, length, 0.0));
    let z = projector.point(Point3::new(0.0, 0.0, length));
    let stroke = |color| Stroke::new(1.4, color);
    painter.line_segment([origin, x], stroke(theme::axis_x()));
    painter.line_segment([origin, y], stroke(theme::axis_y()));
    painter.line_segment([origin, z], stroke(theme::preview_rapid()));
    painter.text(
        x,
        Align2::LEFT_BOTTOM,
        "X",
        FontId::monospace(9.0),
        theme::axis_x(),
    );
    painter.text(
        y,
        Align2::LEFT_BOTTOM,
        "Y",
        FontId::monospace(9.0),
        theme::axis_y(),
    );
    painter.text(
        z,
        Align2::LEFT_BOTTOM,
        "Z",
        FontId::monospace(9.0),
        theme::preview_rapid(),
    );
}

fn draw_paths(
    painter: &Painter,
    preview: &PreviewSession,
    data: &GcodePreview,
    projector: &Projector,
) {
    let state = data.state_at(preview.time_s);
    let active = state.move_index;
    let dim_cut = with_alpha(theme::preview_cut(), 54);
    let dim_rapid = with_alpha(theme::preview_rapid(), 58);

    for path in &data.paths {
        if path.cutting && !preview.show_cut {
            continue;
        }
        if !path.cutting && !preview.show_rapid {
            continue;
        }
        let color = if path.cutting {
            theme::preview_cut()
        } else {
            theme::preview_rapid()
        };
        let dim = if path.cutting { dim_cut } else { dim_rapid };

        let projected: Vec<Pos2> = path
            .points
            .iter()
            .map(|point| projector.point(*point))
            .collect();
        if projected.len() >= 2 {
            painter.add(eframe::egui::Shape::line(projected, Stroke::new(0.75, dim)));
        }

        let executed_end = active.unwrap_or(0).min(path.move_end);
        let mut bright: Vec<Pos2> = Vec::new();
        for index in path.move_start..executed_end {
            for point in &data.moves[index].points {
                let screen = projector.point(*point);
                if bright.last() != Some(&screen) {
                    bright.push(screen);
                }
            }
        }
        if let Some(active) = active
            && active >= path.move_start
            && active < path.move_end
        {
            let screen = projector.point(state.position);
            if bright.last() != Some(&screen) {
                bright.push(screen);
            }
        }
        if bright.len() >= 2 {
            painter.add(eframe::egui::Shape::line(
                bright,
                Stroke::new(if path.cutting { 1.4 } else { 0.9 }, color),
            ));
        } else if bright.len() == 1 && active.is_some() {
            painter.circle_filled(bright[0], 2.0, color);
        }
    }
}

fn draw_tool(painter: &Painter, projector: &Projector, state: &PreviewState) {
    let position = state.position;
    let tip = projector.point(position);
    let surface = projector.surface(position.x, position.y);
    if position.z < -0.01 {
        dashed_line(
            painter,
            tip,
            surface,
            5.0,
            4.0,
            Stroke::new(
                0.8,
                Color32::from_rgba_unmultiplied(
                    theme::preview_cut().r(),
                    theme::preview_cut().g(),
                    theme::preview_cut().b(),
                    150,
                ),
            ),
        );
        painter.circle_stroke(
            surface,
            4.0,
            Stroke::new(0.8, with_alpha(theme::preview_cut(), 160)),
        );
    }

    // Keep the spindle holder a readable size on screen while still pointing
    // along the projected Z axis.
    let up_point = Point3::new(position.x, position.y, position.z + 1.0);
    let up_screen = projector.point(up_point);
    let direction = (up_screen - tip).normalized();
    let holder = if direction.length() < 0.5 {
        tip - vec2(0.0, 52.0)
    } else {
        tip + direction * 52.0
    };
    let holder_direction = (holder - tip).normalized();
    let perpendicular = vec2(-holder_direction.y, holder_direction.x);
    let cone = vec![
        tip + perpendicular * 3.0,
        holder + perpendicular * 6.0,
        holder - perpendicular * 6.0,
        tip - perpendicular * 3.0,
    ];
    painter.add(eframe::egui::Shape::convex_polygon(
        cone,
        with_alpha(theme::panel_raised(), 215),
        Stroke::new(0.8, with_alpha(theme::preview_tool(), 185)),
    ));
    painter.line_segment([tip, holder], Stroke::new(1.1, theme::preview_tool()));
    painter.circle_filled(holder, 5.0, theme::preview_tool());
    painter.circle_filled(tip, 3.2, theme::preview_cut());
    painter.circle_stroke(tip, 7.0, Stroke::new(1.0, theme::preview_tool()));
    let arm = 5.0;
    painter.line_segment(
        [tip - vec2(arm, 0.0), tip + vec2(arm, 0.0)],
        Stroke::new(0.8, with_alpha(theme::preview_tool(), 170)),
    );
    painter.line_segment(
        [tip - vec2(0.0, arm), tip + vec2(0.0, arm)],
        Stroke::new(0.8, with_alpha(theme::preview_tool(), 170)),
    );
}

fn draw_overlay(painter: &Painter, rect: Rect, preview: &PreviewSession, data: &GcodePreview) {
    let state = data.state_at(preview.time_s);
    let mut x = rect.left() + 14.0;
    let y = rect.top() + 14.0;
    x = chip(painter, pos2(x, y), "SIMULATION", theme::copper()).right() + 8.0;
    let motion = if state.cutting {
        ("CUTTING", theme::preview_cut())
    } else if state.move_index.is_some() {
        ("RAPID / TRAVEL", theme::preview_rapid())
    } else {
        ("HOLD", theme::text_muted())
    };
    chip(painter, pos2(x, y), motion.0, motion.1);

    let progress = preview.progress();
    let progress_label = format!(
        "{:.1}%  ·  {} / {}",
        progress * 100.0,
        pcb_cam_core::preview::format_seconds(preview.time_s),
        pcb_cam_core::preview::format_seconds(preview.duration_s()),
    );
    let text = format!(
        "X {:+.3}   Y {:+.3}   Z {:+.3}",
        state.position.x, state.position.y, state.position.z
    );
    let tool = preview.tool(state.tool).map_or_else(
        || "No tool".to_owned(),
        |tool| {
            format!(
                "T{} {} · Ø{:.2} mm",
                tool.number, tool.name, tool.diameter_mm
            )
        },
    );
    let feed = state
        .feed_mm_min
        .map_or_else(|| "feed —".to_owned(), |feed| format!("F {feed:.0} mm/min"));

    let detail = rect.width() >= 430.0;
    let mut lines: Vec<(String, FontId, Color32)> =
        vec![(text, FontId::monospace(12.0), theme::text())];
    if detail {
        lines.push((
            format!("{tool}   {feed}   {progress_label}"),
            FontId::monospace(10.5),
            theme::text_muted(),
        ));
    }
    let panel_height = if detail { 50.0 } else { 28.0 };
    let anchor = pos2(rect.left() + 14.0, rect.top() + 45.0);
    let panel = Rect::from_min_size(
        anchor,
        vec2((rect.width() - 28.0).clamp(200.0, 390.0), panel_height),
    );
    painter.rect_filled(
        panel,
        CornerRadius::same(6),
        Color32::from_rgba_unmultiplied(
            theme::panel_raised().r(),
            theme::panel_raised().g(),
            theme::panel_raised().b(),
            225,
        ),
    );
    painter.rect_stroke(
        panel,
        CornerRadius::same(6),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    for (index, (line, font, color)) in lines.into_iter().enumerate() {
        painter.text(
            anchor + vec2(9.0, 7.0 + index as f32 * 20.0),
            Align2::LEFT_TOP,
            line,
            font,
            color,
        );
    }

    // Legend.
    let legend_y = rect.bottom() - 30.0;
    let mut lx = rect.left() + 14.0;
    lx = legend_chip(painter, pos2(lx, legend_y), "Cutting", theme::preview_cut()).right() + 8.0;
    if rect.width() >= 360.0 {
        lx = legend_chip(
            painter,
            pos2(lx, legend_y),
            "Rapid / travel",
            theme::preview_rapid(),
        )
        .right()
            + 8.0;
        legend_chip(
            painter,
            pos2(lx, legend_y),
            &format!("Z ×{:.0}", preview.z_scale),
            theme::text_muted(),
        );
    }

    if rect.width() >= 520.0 {
        painter.text(
            pos2(rect.right() - 14.0, rect.bottom() - 16.0),
            Align2::RIGHT_BOTTOM,
            "Drag to orbit · Shift+drag to pan · Scroll to zoom · Double-click to fit",
            FontId::proportional(9.5),
            theme::text_faint(),
        );
    }

    if rect.width() >= 320.0 {
        draw_gizmo(painter, rect, preview);
    }
}

fn draw_gizmo(painter: &Painter, rect: Rect, preview: &PreviewSession) {
    let (right, up, _) = preview.camera.basis();
    let origin = pos2(rect.right() - 62.0, rect.bottom() - 62.0);
    let length = 28.0;
    let axes = [
        ("X", Vec3::new(1.0, 0.0, 0.0), theme::axis_x()),
        ("Y", Vec3::new(0.0, 1.0, 0.0), theme::axis_y()),
        ("Z", Vec3::new(0.0, 0.0, 1.0), theme::preview_rapid()),
    ];
    painter.circle_filled(
        origin,
        26.0,
        Color32::from_rgba_unmultiplied(
            theme::panel_raised().r(),
            theme::panel_raised().g(),
            theme::panel_raised().b(),
            210,
        ),
    );
    for (label, axis, color) in axes {
        let end = origin + vec2(right.dot(axis) * length, -up.dot(axis) * length);
        painter.line_segment([origin, end], Stroke::new(1.6, color));
        painter.text(
            end,
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(9.0),
            color,
        );
    }
}

fn draw_view_controls(ui: &mut Ui, rect: Rect, preview: &mut PreviewSession) {
    let max_rect = Rect::from_min_size(
        pos2(rect.right() - 246.0, rect.top() + 12.0),
        vec2(234.0, 28.0),
    );
    ui.scope_builder(UiBuilder::new().max_rect(max_rect), |ui| {
        Frame::new()
            .fill(Color32::from_rgba_unmultiplied(
                theme::panel_raised().r(),
                theme::panel_raised().g(),
                theme::panel_raised().b(),
                235,
            ))
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(6)
            .inner_margin(Margin::same(3))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                ui.horizontal(|ui| {
                    for preset in [CameraPreset::Iso, CameraPreset::Top, CameraPreset::Front] {
                        if ui
                            .add_sized(
                                vec2(38.0, 20.0),
                                Button::new(RichText::new(preset.label()).size(10.0)),
                            )
                            .on_hover_text(format!("{} view", preset.label()))
                            .clicked()
                        {
                            preview.camera.set_preset(preset);
                            preview.fit_pending = true;
                        }
                    }
                    if ui
                        .add_sized(
                            vec2(38.0, 20.0),
                            Button::new(RichText::new("Fit").size(10.0)),
                        )
                        .on_hover_text("Fit toolpath (F)")
                        .clicked()
                    {
                        preview.fit_pending = true;
                    }
                });
            });
    });
}

fn chip(painter: &Painter, anchor: Pos2, text: &str, accent: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::monospace(9.5), theme::text());
    let rect = Rect::from_min_size(anchor, galley.size() + vec2(16.0, 8.0));
    painter.rect_filled(
        rect,
        CornerRadius::same(5),
        Color32::from_rgba_unmultiplied(
            theme::panel_raised().r(),
            theme::panel_raised().g(),
            theme::panel_raised().b(),
            230,
        ),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, with_alpha(accent, 150)),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(8.0, 4.0), galley, theme::text());
    rect
}

fn legend_chip(painter: &Painter, anchor: Pos2, text: &str, accent: Color32) -> Rect {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::proportional(10.0),
        theme::text_muted(),
    );
    let width = galley.size().x + 28.0;
    let rect = Rect::from_min_size(anchor, vec2(width, 20.0));
    painter.rect_filled(
        rect,
        CornerRadius::same(4),
        Color32::from_rgba_unmultiplied(
            theme::panel_raised().r(),
            theme::panel_raised().g(),
            theme::panel_raised().b(),
            210,
        ),
    );
    painter.circle_filled(pos2(rect.left() + 9.0, rect.center().y), 3.5, accent);
    painter.galley(
        pos2(rect.left() + 18.0, rect.center().y - galley.size().y * 0.5),
        galley,
        theme::text_muted(),
    );
    rect
}

fn draw_unavailable(painter: &Painter, rect: Rect, error: Option<&str>) {
    let width = (rect.width() - 64.0).clamp(300.0, 620.0);
    let card = Rect::from_center_size(rect.center(), vec2(width, 250.0));
    painter.rect_filled(card, CornerRadius::same(12), theme::panel_raised());
    painter.rect_stroke(
        card,
        CornerRadius::same(12),
        Stroke::new(1.0, theme::border_strong()),
        StrokeKind::Inside,
    );
    painter.text(
        pos2(card.center().x, card.top() + 34.0),
        Align2::CENTER_CENTER,
        "Preview unavailable",
        FontId::proportional(20.0),
        theme::text(),
    );
    painter.text(
        pos2(card.center().x, card.top() + 60.0),
        Align2::CENTER_CENTER,
        "The G-code could not be generated or parsed.",
        FontId::proportional(11.5),
        theme::text_muted(),
    );
    let message = error.unwrap_or("No preview data.");
    let galley = painter.layout(
        message.to_owned(),
        FontId::monospace(10.5),
        theme::error(),
        width - 56.0,
    );
    painter.galley(
        pos2(card.left() + 28.0, card.top() + 82.0),
        galley,
        theme::error(),
    );
    painter.text(
        pos2(card.center().x, card.bottom() - 30.0),
        Align2::CENTER_CENTER,
        "Go back to the workspace, fix the task, then preview again.",
        FontId::proportional(10.5),
        theme::text_faint(),
    );
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn dashed_line(painter: &Painter, start: Pos2, end: Pos2, dash: f32, gap: f32, stroke: Stroke) {
    let delta = end - start;
    let length = delta.length();
    if length < 1.0 {
        return;
    }
    let direction = delta / length;
    let mut position = 0.0;
    while position < length {
        let next = (position + dash).min(length);
        painter.line_segment(
            [start + direction * position, start + direction * next],
            stroke,
        );
        position = next + gap;
    }
}
