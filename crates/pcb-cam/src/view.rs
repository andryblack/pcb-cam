//! Canvas camera and unit formatting helpers.

use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};
use pcb_cam_core::Unit;

/// Zoom at which the canvas is considered to be at 100 %.
pub(crate) const BASE_ZOOM: f32 = 12.0;
/// Minimum canvas zoom in pixels per millimetre.
pub(crate) const MIN_ZOOM: f32 = 0.5;
/// Maximum canvas zoom in pixels per millimetre.
pub(crate) const MAX_ZOOM: f32 = 800.0;

/// Camera state of the 2D canvas.
#[derive(Debug, Clone)]
pub(crate) struct ViewState {
    /// Pixels per millimetre.
    zoom: f32,
    /// Screen-space offset of the world origin from the centre of the canvas.
    pan: Vec2,
    /// World-space position under the mouse pointer, if it is over the canvas.
    hovered_mm: Option<Pos2>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: BASE_ZOOM,
            pan: Vec2::ZERO,
            hovered_mm: None,
        }
    }
}

impl ViewState {
    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }

    pub(crate) fn pan_by(&mut self, delta: Vec2) {
        self.pan += delta;
    }

    pub(crate) fn hovered_mm(&self) -> Option<Pos2> {
        self.hovered_mm
    }

    pub(crate) fn set_hovered_mm(&mut self, position: Option<Pos2>) {
        self.hovered_mm = position;
    }

    pub(crate) fn zoom_percent(&self) -> f32 {
        self.zoom / BASE_ZOOM * 100.0
    }

    /// Reset pan and zoom to the default view.
    pub(crate) fn reset(&mut self) {
        self.zoom = BASE_ZOOM;
        self.pan = Vec2::ZERO;
        self.hovered_mm = None;
    }

    /// Fit a bounding box into the canvas, leaving a margin for overlays.
    pub(crate) fn fit_bounds(&mut self, bounds: pcb_cam_core::Bounds, canvas: Rect, margin: f32) {
        let available = vec2(
            (canvas.width() - margin * 2.0).max(24.0),
            (canvas.height() - margin * 2.0).max(24.0),
        );
        let width = bounds.width().max(0.001) as f32;
        let height = bounds.height().max(0.001) as f32;
        self.zoom = (available.x / width)
            .min(available.y / height)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let center = bounds.center();
        self.pan = vec2(
            -(center.x as f32) * self.zoom,
            (center.y as f32) * self.zoom,
        );
        self.hovered_mm = None;
    }

    /// Zoom by `factor` while keeping the world point under `anchor` fixed.
    pub(crate) fn zoom_at(&mut self, factor: f32, anchor: Pos2, canvas: Rect) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }

        let before = self.screen_to_world(anchor, canvas);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.world_to_screen(before, canvas);
        self.pan += anchor - after;
    }

    pub(crate) fn world_to_screen(&self, world: Pos2, canvas: Rect) -> Pos2 {
        let origin = canvas.center() + self.pan;
        pos2(
            origin.x + world.x * self.zoom,
            origin.y - world.y * self.zoom,
        )
    }

    pub(crate) fn screen_to_world(&self, screen: Pos2, canvas: Rect) -> Pos2 {
        let origin = canvas.center() + self.pan;
        pos2(
            (screen.x - origin.x) / self.zoom,
            -(screen.y - origin.y) / self.zoom,
        )
    }
}

/// Pick a readable grid spacing for the current zoom.
pub(crate) fn grid_step_mm(zoom: f32) -> f64 {
    const NICE_STEPS: [f64; 23] = [
        0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0,
        100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0,
    ];
    const TARGET_PIXELS: f64 = 12.0;

    NICE_STEPS
        .iter()
        .copied()
        .find(|step| step * f64::from(zoom) >= TARGET_PIXELS)
        .unwrap_or(NICE_STEPS[NICE_STEPS.len() - 1])
}

/// Pick a round scale-bar length that is close to `target_pixels` wide.
pub(crate) fn scale_bar_mm(zoom: f32, target_pixels: f32) -> f64 {
    let raw = f64::from(target_pixels / zoom.max(f32::EPSILON));
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }

    let exponent = raw.log10().floor();
    let base = 10_f64.powf(exponent);
    for multiplier in [5.0, 2.0, 1.0] {
        let candidate = base * multiplier;
        if candidate <= raw {
            return candidate;
        }
    }
    base
}

/// Format a millimetre value using the active display unit.
pub(crate) fn format_length(mm: f32, unit: Unit) -> String {
    let value = unit.from_mm(f64::from(mm));
    let symbol = unit.symbol();
    if value.abs() >= 10.0 {
        format!("{value:.1} {symbol}")
    } else if value.abs() >= 0.01 {
        format!("{value:.2} {symbol}")
    } else {
        format!("{value:.3} {symbol}")
    }
}

/// Format a coordinate with a sign and a fixed precision.
pub(crate) fn format_coordinate(mm: f32, unit: Unit) -> String {
    format!("{:+.3}", unit.from_mm(f64::from(mm)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_and_world_round_trip() {
        let view = ViewState::default();
        let canvas = Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 600.0));
        let world = pos2(12.5, -4.25);
        let screen = view.world_to_screen(world, canvas);
        let round_trip = view.screen_to_world(screen, canvas);
        assert!((round_trip.x - world.x).abs() < 1e-4);
        assert!((round_trip.y - world.y).abs() < 1e-4);
    }

    #[test]
    fn zooming_keeps_anchor_fixed() {
        let mut view = ViewState::default();
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let anchor = pos2(510.0, 260.0);
        let before = view.screen_to_world(anchor, canvas);
        view.zoom_at(2.5, anchor, canvas);
        let after = view.screen_to_world(anchor, canvas);
        assert!((before.x - after.x).abs() < 1e-3);
        assert!((before.y - after.y).abs() < 1e-3);
    }

    #[test]
    fn grid_step_grows_when_zooming_out() {
        assert!(grid_step_mm(10.0) >= 1.0);
        assert!(grid_step_mm(0.5) > grid_step_mm(10.0));
    }

    #[test]
    fn fitting_centres_and_contains_the_bounds() {
        let mut view = ViewState::default();
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let bounds = pcb_cam_core::Bounds {
            min: pcb_cam_core::Point::new(10.0, 20.0),
            max: pcb_cam_core::Point::new(90.0, 70.0),
        };
        view.fit_bounds(bounds, canvas, 40.0);
        let top_left = view.world_to_screen(pos2(bounds.min.x as f32, bounds.min.y as f32), canvas);
        let bottom_right =
            view.world_to_screen(pos2(bounds.max.x as f32, bounds.max.y as f32), canvas);
        assert!(canvas.contains(top_left));
        assert!(canvas.contains(bottom_right));
    }

    #[test]
    fn formatting_respects_units() {
        assert_eq!(format_coordinate(25.4, Unit::Inch), "+1.000");
        assert_eq!(format_coordinate(1.0, Unit::Millimeter), "+1.000");
    }
}
