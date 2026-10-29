//! Interactive 2D board canvas.

use eframe::egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Mesh, Painter, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use pcb_cam_core::{
    Aperture, ApertureHole, ApertureShape, Bounds, ExcellonLayer, GerberLayer, Layer,
    LayerGeometry, OperationKind, Path, Polarity, Primitive,
};

use crate::app::{PbcCam, Selection};
use crate::theme;
use crate::ui::widgets;
use crate::view::{self, ViewState};

/// Draw the central canvas and handle its camera controls.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let available = ui.available_size();
    let (response, _) = ui.allocate_painter(available, Sense::click_and_drag());
    let rect = response.rect;
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }

    if app.fit_requested {
        app.fit_requested = false;
        if let Some(bounds) = app.project().bounds(true) {
            app.view.fit_bounds(bounds, rect, 44.0);
        } else {
            app.view.reset();
        }
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

    let selected_layer = match app.selection {
        Some(Selection::Layer(id)) => Some(id),
        _ => None,
    };
    let visible_layers: Vec<&Layer> = app
        .project()
        .layers()
        .iter()
        .filter(|layer| layer.visible())
        .collect();

    for layer in visible_layers
        .iter()
        .copied()
        .filter(|layer| Some(layer.id()) != selected_layer)
    {
        draw_layer(&painter, rect, &app.view, layer, false);
    }
    if let Some(layer) = visible_layers
        .iter()
        .copied()
        .find(|layer| Some(layer.id()) == selected_layer)
    {
        draw_layer(&painter, rect, &app.view, layer, true);
    }

    draw_task_overlay(&painter, rect, app);

    let empty_card = if app.project().is_empty() {
        Some(draw_empty_state(&painter, rect))
    } else if visible_layers.is_empty() {
        Some(draw_hidden_state(&painter, rect))
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
        app.fit_view();
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

fn draw_layer(painter: &Painter, rect: Rect, view: &ViewState, layer: &Layer, selected: bool) {
    let [r, g, b] = layer.color();
    let color = if selected {
        Color32::from_rgb(r, g, b)
    } else {
        // Pre-blend toward the canvas instead of drawing translucent strokes.
        // Overlapping caps/joins must not become darker or lighter.
        blend_over_canvas(r, g, b, 220)
    };
    match layer.geometry() {
        LayerGeometry::Gerber(gerber) => {
            draw_gerber(painter, rect, view, gerber, color);
        }
        LayerGeometry::Excellon(excellon) => {
            draw_excellon(painter, rect, view, excellon, color);
        }
    }
}

fn draw_gerber(
    painter: &Painter,
    rect: Rect,
    view: &ViewState,
    gerber: &GerberLayer,
    color: Color32,
) {
    for primitive in &gerber.primitives {
        let Some(bounds) = primitive.bounds() else {
            continue;
        };
        if !bounds_visible(bounds, view, rect) {
            continue;
        }
        let primitive_color = match primitive {
            Primitive::Stroke { polarity, .. }
            | Primitive::Flash { polarity, .. }
            | Primitive::Region { polarity, .. } => {
                if *polarity == Polarity::Clear {
                    theme::CANVAS
                } else {
                    color
                }
            }
        };

        match primitive {
            Primitive::Stroke { path, aperture, .. } => {
                let points: Vec<Pos2> = path
                    .points
                    .iter()
                    .map(|point| view.world_to_screen(pos2(point.x as f32, point.y as f32), rect))
                    .collect();
                let width = (aperture_stroke_width(aperture) * view.zoom()).max(0.8);
                if let Some(mesh) =
                    stroke_mesh(&points, path.closed, width.min(80.0), primitive_color)
                {
                    painter.add(Shape::mesh(mesh));
                }
            }
            Primitive::Flash {
                position,
                aperture,
                rotation_degrees,
                ..
            } => {
                draw_flash(
                    painter,
                    rect,
                    view,
                    *position,
                    aperture,
                    *rotation_degrees,
                    primitive_color,
                );
            }
            Primitive::Region { contours, .. } => {
                draw_region(painter, rect, view, contours, primitive_color);
            }
        }
    }
}

fn blend_over_canvas(r: u8, g: u8, b: u8, alpha: u8) -> Color32 {
    let a = f32::from(alpha) / 255.0;
    let canvas = theme::CANVAS;
    let mix = |foreground: u8, background: u8| {
        (f32::from(foreground) * a + f32::from(background) * (1.0 - a)).round() as u8
    };
    Color32::from_rgb(mix(r, canvas.r()), mix(g, canvas.g()), mix(b, canvas.b()))
}

/// Tessellate a Gerber stroke as a round-capped, round-joined band.
///
/// Epaint's thick paths use miter joins and cut off sharp corners, which is
/// wrong for a circular Gerber aperture: the aperture centre traces the path
/// and the swept area is the Minkowski sum with a disc.
fn stroke_mesh(points: &[Pos2], closed: bool, width: f32, color: Color32) -> Option<Mesh> {
    if points.len() < 2 || !width.is_finite() || width <= 0.0 {
        return None;
    }
    let radius = width * 0.5;
    let mut mesh = Mesh::default();

    let mut add_quad = |a: Pos2, b: Pos2| {
        let delta = b - a;
        if delta.length_sq() < 1e-12 {
            return;
        }
        let normal = delta.normalized().rot90() * radius;
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(a + normal, color);
        mesh.colored_vertex(b + normal, color);
        mesh.colored_vertex(b - normal, color);
        mesh.colored_vertex(a - normal, color);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    };

    for pair in points.windows(2) {
        add_quad(pair[0], pair[1]);
    }
    if closed && points.len() > 2 {
        add_quad(points[points.len() - 1], points[0]);
    }

    let segments = if radius < 1.5 {
        6
    } else if radius < 4.0 {
        10
    } else if radius < 10.0 {
        16
    } else {
        24
    };
    let step = std::f32::consts::TAU / segments as f32;
    for center in points {
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(*center, color);
        for index in 0..segments {
            let angle = index as f32 * step;
            mesh.colored_vertex(
                pos2(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                ),
                color,
            );
        }
        for index in 0..segments {
            let next = (index + 1) % segments;
            mesh.add_triangle(base, base + 1 + index, base + 1 + next);
        }
    }

    Some(mesh)
}

fn draw_region(painter: &Painter, rect: Rect, view: &ViewState, contours: &[Path], color: Color32) {
    let mut polygons: Vec<Vec<Pos2>> = contours
        .iter()
        .filter_map(|contour| {
            let mut points: Vec<Pos2> = contour
                .points
                .iter()
                .map(|point| view.world_to_screen(pos2(point.x as f32, point.y as f32), rect))
                .collect();
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            (points.len() >= 3).then_some(points)
        })
        .collect();
    if polygons.is_empty() {
        return;
    }

    // Gerber regions may contain an outer contour and holes. Earcut expects
    // the outer ring first; use the largest ring as the outer one.
    polygons.sort_by(|a, b| polygon_area(b).abs().total_cmp(&polygon_area(a).abs()));
    let mut data: Vec<f32> = Vec::new();
    let mut holes: Vec<usize> = Vec::new();
    for (index, polygon) in polygons.iter().enumerate() {
        if index > 0 {
            holes.push(data.len() / 2);
        }
        for point in polygon {
            data.push(point.x);
            data.push(point.y);
        }
    }
    let Ok(indices) = earcutr::earcut(&data, &holes, 2) else {
        return;
    };
    if indices.is_empty() {
        return;
    }

    let mut mesh = Mesh::default();
    for [x, y] in data.as_chunks::<2>().0 {
        mesh.colored_vertex(pos2(*x, *y), color);
    }
    for [a, b, c] in indices.as_chunks::<3>().0 {
        mesh.add_triangle(*a as u32, *b as u32, *c as u32);
    }
    painter.add(Shape::mesh(mesh));
}

fn polygon_area(points: &[Pos2]) -> f32 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        area += a.x * b.y - b.x * a.y;
    }
    area * 0.5
}

fn draw_flash(
    painter: &Painter,
    rect: Rect,
    view: &ViewState,
    position: pcb_cam_core::Point,
    aperture: &Aperture,
    rotation_degrees: f64,
    color: Color32,
) {
    let center = view.world_to_screen(pos2(position.x as f32, position.y as f32), rect);
    if !rect.expand(100.0).contains(center) {
        return;
    }
    match aperture.shape {
        ApertureShape::Circle { diameter } => {
            let radius = (diameter as f32 * 0.5 * view.zoom()).max(0.7);
            painter.circle_filled(center, radius, color);
            draw_hole(painter, center, aperture.hole.as_ref(), view, radius);
        }
        ApertureShape::Rectangle { .. }
        | ApertureShape::Obround { .. }
        | ApertureShape::Polygon { .. } => {
            let world_points = aperture.outline(position, rotation_degrees);
            let points: Vec<Pos2> = world_points
                .iter()
                .map(|point| view.world_to_screen(pos2(point.x as f32, point.y as f32), rect))
                .collect();
            if points.len() >= 3 {
                painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
            }
            draw_rectangular_hole(painter, center, aperture.hole.as_ref(), view);
        }
    }
}

fn draw_hole(
    painter: &Painter,
    center: Pos2,
    hole: Option<&ApertureHole>,
    view: &ViewState,
    outer_radius: f32,
) {
    match hole {
        Some(ApertureHole::Circle { diameter }) => {
            let radius = (*diameter as f32 * 0.5 * view.zoom()).max(0.4);
            painter.circle_filled(center, radius.min(outer_radius), theme::CANVAS);
        }
        Some(ApertureHole::Rectangle { width, height }) => {
            let size = vec2(
                (*width as f32 * view.zoom()).max(0.8),
                (*height as f32 * view.zoom()).max(0.8),
            );
            painter.rect_filled(
                Rect::from_center_size(center, size),
                CornerRadius::ZERO,
                theme::CANVAS,
            );
        }
        None => {}
    }
}

fn draw_rectangular_hole(
    painter: &Painter,
    center: Pos2,
    hole: Option<&ApertureHole>,
    view: &ViewState,
) {
    if let Some(ApertureHole::Circle { diameter }) = hole {
        let radius = (*diameter as f32 * 0.5 * view.zoom()).max(0.4);
        painter.circle_filled(center, radius, theme::CANVAS);
    }
}

fn draw_excellon(
    painter: &Painter,
    rect: Rect,
    view: &ViewState,
    excellon: &ExcellonLayer,
    color: Color32,
) {
    for hole in &excellon.holes {
        let center =
            view.world_to_screen(pos2(hole.position.x as f32, hole.position.y as f32), rect);
        if !rect.expand(20.0).contains(center) {
            continue;
        }
        let radius = (hole.diameter_mm as f32 * 0.5 * view.zoom()).max(1.2);
        painter.circle_filled(center, radius, color);
        if radius > 2.2 {
            painter.circle_filled(center, radius - 1.3, theme::CANVAS);
        }
    }

    for slot in &excellon.slots {
        let start = view.world_to_screen(pos2(slot.start.x as f32, slot.start.y as f32), rect);
        let end = view.world_to_screen(pos2(slot.end.x as f32, slot.end.y as f32), rect);
        if !rect.expand(20.0).contains(start) && !rect.expand(20.0).contains(end) {
            continue;
        }
        let width = (slot.diameter_mm as f32 * view.zoom()).max(1.4);
        draw_slot(painter, start, end, width, color);
    }
}

fn draw_slot(painter: &Painter, start: Pos2, end: Pos2, width: f32, color: Color32) {
    let radius = width * 0.5;
    painter.line_segment([start, end], Stroke::new(width, color));
    painter.circle_filled(start, radius, color);
    painter.circle_filled(end, radius, color);

    let inner = width - 2.0;
    if inner > 0.8 {
        let inner_radius = inner * 0.5;
        painter.line_segment([start, end], Stroke::new(inner, theme::CANVAS));
        painter.circle_filled(start, inner_radius, theme::CANVAS);
        painter.circle_filled(end, inner_radius, theme::CANVAS);
    }
}

fn aperture_stroke_width(aperture: &Aperture) -> f32 {
    match aperture.shape {
        ApertureShape::Circle { diameter } => diameter as f32,
        ApertureShape::Rectangle { width, height } | ApertureShape::Obround { width, height } => {
            width.min(height) as f32
        }
        ApertureShape::Polygon { diameter, .. } => diameter as f32,
    }
    .max(0.01)
}

fn bounds_visible(bounds: Bounds, view: &ViewState, rect: Rect) -> bool {
    let min = view.world_to_screen(pos2(bounds.min.x as f32, bounds.max.y as f32), rect);
    let max = view.world_to_screen(pos2(bounds.max.x as f32, bounds.min.y as f32), rect);
    Rect::from_min_max(min, max).intersects(rect.expand(2.0))
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

fn draw_task_overlay(painter: &Painter, rect: Rect, app: &PbcCam) {
    let Some(Selection::Task(id)) = app.selection else {
        return;
    };
    let Some(task) = app.project().task(id) else {
        return;
    };
    let color = if task.enabled() {
        theme::TASK_OVERLAY
    } else {
        Color32::from_rgba_unmultiplied(255, 82, 82, 110)
    };
    let Some(layer) = app.project().layer(task.source_layer()) else {
        return;
    };

    match task.kind() {
        OperationKind::Drill(drill) => {
            let LayerGeometry::Excellon(excellon) = layer.geometry() else {
                return;
            };
            let selected =
                |tool: u32| drill.source_tools.is_empty() || drill.source_tools.contains(&tool);
            for hole in &excellon.holes {
                if selected(hole.tool) {
                    draw_drill_position(
                        painter,
                        rect,
                        &app.view,
                        hole.position.x,
                        hole.position.y,
                        color,
                    );
                }
            }
            for slot in &excellon.slots {
                if !selected(slot.tool) {
                    continue;
                }
                let start = app
                    .view
                    .world_to_screen(pos2(slot.start.x as f32, slot.start.y as f32), rect);
                let end = app
                    .view
                    .world_to_screen(pos2(slot.end.x as f32, slot.end.y as f32), rect);
                if !rect.expand(30.0).contains(start) && !rect.expand(30.0).contains(end) {
                    continue;
                }
                painter.line_segment(
                    [start, end],
                    Stroke::new(4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 150)),
                );
                painter.line_segment([start, end], Stroke::new(1.8, color));
                draw_drill_cross(painter, start, color);
                draw_drill_cross(painter, end, color);
            }
        }
        OperationKind::SpotDrill(spot) => {
            let LayerGeometry::Excellon(excellon) = layer.geometry() else {
                return;
            };
            for hole in &excellon.holes {
                if spot.source_tools.is_empty() || spot.source_tools.contains(&hole.tool) {
                    draw_drill_position(
                        painter,
                        rect,
                        &app.view,
                        hole.position.x,
                        hole.position.y,
                        color,
                    );
                }
            }
        }
        OperationKind::HoleMill(mill) => {
            let LayerGeometry::Excellon(excellon) = layer.geometry() else {
                return;
            };
            let Some(tool) = app.session.tool_library().tool(mill.tool) else {
                return;
            };
            let selected = |tool_number: u32| {
                mill.source_tools.is_empty() || mill.source_tools.contains(&tool_number)
            };
            for hole in &excellon.holes {
                if !selected(hole.tool) {
                    continue;
                }
                let center = app
                    .view
                    .world_to_screen(pos2(hole.position.x as f32, hole.position.y as f32), rect);
                if !rect.expand(30.0).contains(center) {
                    continue;
                }
                let path_radius = ((hole.diameter_mm - tool.diameter_mm())
                    * 0.5
                    * f64::from(app.view.zoom())) as f32;
                if path_radius > 0.5 {
                    painter.circle_stroke(center, path_radius, Stroke::new(1.4, color));
                    draw_drill_cross(painter, center, color);
                } else {
                    draw_drill_position(
                        painter,
                        rect,
                        &app.view,
                        hole.position.x,
                        hole.position.y,
                        color,
                    );
                }
            }
            for slot in &excellon.slots {
                if !selected(slot.tool) {
                    continue;
                }
                let start = app
                    .view
                    .world_to_screen(pos2(slot.start.x as f32, slot.start.y as f32), rect);
                let end = app
                    .view
                    .world_to_screen(pos2(slot.end.x as f32, slot.end.y as f32), rect);
                painter.line_segment([start, end], Stroke::new(1.8, color));
                draw_drill_cross(painter, start, color);
                draw_drill_cross(painter, end, color);
            }
        }
        OperationKind::Cutout(cutout) => {
            let LayerGeometry::Gerber(gerber) = layer.geometry() else {
                return;
            };
            for path in cutout.toolpaths(gerber) {
                draw_task_path(painter, rect, &app.view, &path, color);
            }
        }
        OperationKind::Engrave(_) => {
            if let Some(paths) = app.clearance_preview_paths(task.id()) {
                for path in paths {
                    draw_task_path(painter, rect, &app.view, path, color);
                }
            }
        }
    }
}

fn draw_task_path(painter: &Painter, rect: Rect, view: &ViewState, path: &Path, color: Color32) {
    let mut points: Vec<Pos2> = path
        .points
        .iter()
        .map(|point| view.world_to_screen(pos2(point.x as f32, point.y as f32), rect))
        .collect();
    if path.closed
        && let Some(first) = points.first().copied()
    {
        points.push(first);
    }
    if points.len() < 2 {
        return;
    }
    painter.add(Shape::line(
        points.clone(),
        Stroke::new(4.5, Color32::from_rgba_unmultiplied(0, 0, 0, 165)),
    ));
    painter.add(Shape::line(points, Stroke::new(1.8, color)));
}

fn draw_drill_position(
    painter: &Painter,
    rect: Rect,
    view: &ViewState,
    x: f64,
    y: f64,
    color: Color32,
) {
    let center = view.world_to_screen(pos2(x as f32, y as f32), rect);
    if !rect.expand(30.0).contains(center) {
        return;
    }
    painter.circle_stroke(center, 5.0, Stroke::new(1.4, color));
    draw_drill_cross(painter, center, color);
}

fn draw_drill_cross(painter: &Painter, center: Pos2, color: Color32) {
    let arm = 8.0;
    painter.line_segment(
        [
            pos2(center.x - arm, center.y),
            pos2(center.x + arm, center.y),
        ],
        Stroke::new(1.4, color),
    );
    painter.line_segment(
        [
            pos2(center.x, center.y - arm),
            pos2(center.x, center.y + arm),
        ],
        Stroke::new(1.4, color),
    );
}

fn draw_empty_state(painter: &Painter, rect: Rect) -> Rect {
    draw_message_card(
        painter,
        rect,
        "No fabrication data",
        "Import Gerber copper, outline or silkscreen layers",
        "and an Excellon drill file to start the job.",
        "PCB-CAM  /  GERBER + EXCELLON  >  CNC",
    )
}

fn draw_hidden_state(painter: &Painter, rect: Rect) -> Rect {
    draw_message_card(
        painter,
        rect,
        "All layers are hidden",
        "Use the eye checkboxes in the browser",
        "to make a source layer visible again.",
        "PCB-CAM  /  LAYER VISIBILITY",
    )
}

fn draw_message_card(
    painter: &Painter,
    rect: Rect,
    title: &str,
    line_one: &str,
    line_two: &str,
    footer: &str,
) -> Rect {
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
        title,
        FontId::proportional(19.0),
        theme::TEXT,
    );
    painter.text(
        pos2(card.center().x, card.top() + 125.0),
        Align2::CENTER_CENTER,
        line_one,
        FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
    painter.text(
        pos2(card.center().x, card.top() + 144.0),
        Align2::CENTER_CENTER,
        line_two,
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
        footer,
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
                app.project().units(),
            )
        );
        chip(painter, pos2(x, y), &grid_label);
    }

    let jobs = format!(
        "{} LAYERS  /  {} OBJECTS",
        app.project().visible_layer_count(),
        app.project().object_count()
    );
    chip_right(painter, rect, &jobs);

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

fn chip_right(painter: &Painter, rect: Rect, text: &str) {
    let galley =
        painter.layout_no_wrap(text.to_owned(), FontId::monospace(10.0), theme::TEXT_MUTED);
    let width = galley.size().x + 16.0;
    let anchor = pos2(rect.right() - 14.0 - width, rect.top() + 14.0);
    let chip_rect = Rect::from_min_size(anchor, galley.size() + vec2(16.0, 8.0));
    painter.rect_filled(
        chip_rect,
        CornerRadius::same(5),
        Color32::from_rgba_unmultiplied(17, 22, 28, 236),
    );
    painter.rect_stroke(
        chip_rect,
        CornerRadius::same(5),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Inside,
    );
    painter.galley(chip_rect.min + vec2(8.0, 4.0), galley, theme::TEXT_MUTED);
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
        view::format_length(length_mm as f32, app.project().units()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_mesh_adds_round_cap_at_the_end() {
        let mesh = stroke_mesh(
            &[pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)],
            false,
            4.0,
            Color32::WHITE,
        )
        .expect("mesh");
        assert!(!mesh.indices.is_empty());
        assert!(
            mesh.vertices
                .iter()
                .any(|vertex| (vertex.pos - pos2(12.0, 0.0)).length() < 0.01),
            "round join/cap vertex missing"
        );
    }

    #[test]
    fn blend_over_canvas_is_opaque() {
        let color = blend_over_canvas(227, 164, 94, 220);
        assert_eq!(color.a(), 255);
    }
}
