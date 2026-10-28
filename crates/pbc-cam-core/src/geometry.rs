//! Two-dimensional geometry produced by fabrication-file importers.
//!
//! All coordinates are millimetres with Y pointing up. Formats are flattened into
//! polylines and primitive shapes at import time; the original file units are kept
//! on the layer for display.

use std::f64::consts::TAU;

/// A point or vector in board space, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Point {
    /// The origin.
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    /// Construct a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Distance to another point.
    pub fn distance(self, other: Self) -> f64 {
        (other.x - self.x).hypot(other.y - self.y)
    }

    /// Midpoint between two points.
    pub fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) * 0.5, (self.y + other.y) * 0.5)
    }

    /// Rotate around the origin by `radians`.
    pub fn rotated(self, radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }

    /// Rotate around `center` by `radians`.
    pub fn rotated_around(self, center: Self, radians: f64) -> Self {
        center + (self - center).rotated(radians)
    }

    /// Translate the point.
    pub fn translated(self, delta: Self) -> Self {
        Self::new(self.x + delta.x, self.y + delta.y)
    }

    /// Whether both components are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl core::ops::Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl core::ops::Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl core::ops::Mul<f64> for Point {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Lower-left corner.
    pub min: Point,
    /// Upper-right corner.
    pub max: Point,
}

impl Bounds {
    /// Build bounds from points, returning `None` for an empty or non-finite input.
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        let mut iter = points.into_iter().filter(|point| point.is_finite());
        let first = iter.next()?;
        let mut bounds = Self {
            min: first,
            max: first,
        };
        for point in iter {
            bounds.include(point);
        }
        Some(bounds)
    }

    /// Include a point.
    pub fn include(&mut self, point: Point) {
        if point.is_finite() {
            self.min.x = self.min.x.min(point.x);
            self.min.y = self.min.y.min(point.y);
            self.max.x = self.max.x.max(point.x);
            self.max.y = self.max.y.max(point.y);
        }
    }

    /// Merge with another bounds.
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// Expand by a margin on all sides.
    pub fn expanded(self, margin: f64) -> Self {
        let margin = margin.abs();
        Self {
            min: Point::new(self.min.x - margin, self.min.y - margin),
            max: Point::new(self.max.x + margin, self.max.y + margin),
        }
    }

    /// Width of the box.
    pub fn width(self) -> f64 {
        (self.max.x - self.min.x).max(0.0)
    }

    /// Height of the box.
    pub fn height(self) -> f64 {
        (self.max.y - self.min.y).max(0.0)
    }

    /// Centre of the box.
    pub fn center(self) -> Point {
        self.min.midpoint(self.max)
    }

    /// Whether a point lies inside the box.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

/// A connected polyline. `closed` means the last point connects back to the first.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Vertices in order.
    pub points: Vec<Point>,
    /// Whether the path is closed.
    pub closed: bool,
}

impl Path {
    /// Create an open path.
    pub fn new(points: Vec<Point>) -> Self {
        Self {
            points,
            closed: false,
        }
    }

    /// Create a closed path.
    pub fn closed(points: Vec<Point>) -> Self {
        Self {
            points,
            closed: true,
        }
    }

    /// Bounding box of the vertices.
    pub fn bounds(&self) -> Option<Bounds> {
        Bounds::from_points(self.points.iter().copied())
    }

    /// Total length of the path.
    pub fn length(&self) -> f64 {
        let mut length = 0.0;
        for pair in self.points.windows(2) {
            if let [a, b] = pair {
                length += a.distance(*b);
            }
        }
        if self.closed
            && self.points.len() > 2
            && let (Some(first), Some(last)) = (self.points.first(), self.points.last())
        {
            length += first.distance(*last);
        }
        length
    }

    /// Whether the path has enough points to draw.
    pub fn is_drawable(&self) -> bool {
        self.points.len() >= 2
    }
}

/// Standard Gerber aperture shapes.
#[derive(Debug, Clone, PartialEq)]
pub enum ApertureShape {
    /// Round pad or round drawing tool.
    Circle { diameter: f64 },
    /// Rectangular pad.
    Rectangle { width: f64, height: f64 },
    /// Stadium-shaped pad.
    Obround { width: f64, height: f64 },
    /// Regular polygon pad.
    Polygon {
        diameter: f64,
        vertices: u32,
        rotation_degrees: f64,
    },
}

/// A hole inside a flashed aperture.
#[derive(Debug, Clone, PartialEq)]
pub enum ApertureHole {
    /// Round hole.
    Circle { diameter: f64 },
    /// Rectangular hole.
    Rectangle { width: f64, height: f64 },
}

/// A Gerber aperture.
#[derive(Debug, Clone, PartialEq)]
pub struct Aperture {
    /// Outer shape.
    pub shape: ApertureShape,
    /// Optional hole through the pad.
    pub hole: Option<ApertureHole>,
}

impl Aperture {
    /// Round drawing aperture.
    pub fn circle(diameter: f64) -> Self {
        Self {
            shape: ApertureShape::Circle { diameter },
            hole: None,
        }
    }

    /// Radius of a circle that contains the aperture at the origin.
    pub fn bounding_radius(&self) -> f64 {
        match self.shape {
            ApertureShape::Circle { diameter } => diameter.abs() * 0.5,
            ApertureShape::Rectangle { width, height }
            | ApertureShape::Obround { width, height } => (width.abs().hypot(height.abs())) * 0.5,
            ApertureShape::Polygon { diameter, .. } => diameter.abs() * 0.5,
        }
    }

    /// Outline vertices of the aperture around `center`.
    pub fn outline(&self, center: Point, rotation_degrees: f64) -> Vec<Point> {
        let rotation = rotation_degrees.to_radians();
        let mut points = match self.shape {
            ApertureShape::Circle { diameter } => {
                let radius = diameter.abs() * 0.5;
                let segments = circle_segments(radius, 0.01);
                (0..segments)
                    .map(|index| {
                        let angle = TAU * index as f64 / segments as f64;
                        center + Point::new(angle.cos() * radius, angle.sin() * radius)
                    })
                    .collect()
            }
            ApertureShape::Rectangle { width, height } => {
                let (half_w, half_h) = (width.abs() * 0.5, height.abs() * 0.5);
                [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
                    .into_iter()
                    .map(|(x, y)| center + Point::new(x * half_w, y * half_h))
                    .collect()
            }
            ApertureShape::Obround { width, height } => {
                obround_outline(center, width.abs(), height.abs())
            }
            ApertureShape::Polygon {
                diameter,
                vertices,
                rotation_degrees: shape_rotation,
            } => {
                let radius = diameter.abs() * 0.5;
                let count = vertices.clamp(3, 64);
                let start = (rotation_degrees + shape_rotation).to_radians();
                (0..count)
                    .map(|index| {
                        let angle = start + TAU * index as f64 / count as f64;
                        center + Point::new(angle.cos() * radius, angle.sin() * radius)
                    })
                    .collect()
            }
        };

        if matches!(
            self.shape,
            ApertureShape::Rectangle { .. } | ApertureShape::Obround { .. }
        ) && rotation != 0.0
        {
            for point in &mut points {
                *point = point.rotated_around(center, rotation);
            }
        }

        points
    }
}

/// Polarity of a Gerber primitive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Polarity {
    /// Adds material.
    #[default]
    Dark,
    /// Removes material.
    Clear,
}

/// A flattened Gerber drawing primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// A stroked path.
    Stroke {
        /// Centreline.
        path: Path,
        /// Drawing aperture.
        aperture: Aperture,
        /// Whether it adds or removes material.
        polarity: Polarity,
    },
    /// A flashed aperture.
    Flash {
        /// Centre of the flash.
        position: Point,
        /// Aperture shape.
        aperture: Aperture,
        /// Rotation in degrees.
        rotation_degrees: f64,
        /// Whether it adds or removes material.
        polarity: Polarity,
    },
    /// A filled region, possibly with holes described by additional contours.
    Region {
        /// Closed contours.
        contours: Vec<Path>,
        /// Whether it adds or removes material.
        polarity: Polarity,
    },
}

impl Primitive {
    /// Bounding box of this primitive.
    pub fn bounds(&self) -> Option<Bounds> {
        match self {
            Self::Stroke { path, aperture, .. } => path
                .bounds()
                .map(|bounds| bounds.expanded(aperture.bounding_radius())),
            Self::Flash {
                position, aperture, ..
            } => Some(
                Bounds {
                    min: *position,
                    max: *position,
                }
                .expanded(aperture.bounding_radius()),
            ),
            Self::Region { contours, .. } => Bounds::from_points(
                contours
                    .iter()
                    .flat_map(|contour| contour.points.iter().copied()),
            ),
        }
    }

    /// Whether the primitive adds material.
    pub fn is_dark(&self) -> bool {
        match self {
            Self::Stroke { polarity, .. }
            | Self::Flash { polarity, .. }
            | Self::Region { polarity, .. } => *polarity == Polarity::Dark,
        }
    }
}

/// Parsed Gerber data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GerberLayer {
    /// Flattened drawing primitives in file order.
    pub primitives: Vec<Primitive>,
    /// Units declared by the file.
    pub source_units: Option<crate::Unit>,
    /// Non-fatal parser and importer messages.
    pub warnings: Vec<String>,
}

impl GerberLayer {
    /// Bounding box of all primitives.
    pub fn bounds(&self) -> Option<Bounds> {
        self.primitives
            .iter()
            .filter_map(Primitive::bounds)
            .reduce(Bounds::union)
    }

    /// Number of top-level drawing primitives.
    pub fn object_count(&self) -> usize {
        self.primitives.len()
    }
}

/// Tool definition embedded in an Excellon file.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillTool {
    /// Tool number (`T1` = 1).
    pub number: u32,
    /// Drill diameter in millimetres.
    pub diameter_mm: f64,
}

/// A drilled hole.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillHit {
    /// Hole centre.
    pub position: Point,
    /// Tool number from the source file.
    pub tool: u32,
    /// Drill diameter in millimetres.
    pub diameter_mm: f64,
}

/// A routed slot.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillSlot {
    /// Start of the slot.
    pub start: Point,
    /// End of the slot.
    pub end: Point,
    /// Tool number from the source file.
    pub tool: u32,
    /// Tool diameter in millimetres.
    pub diameter_mm: f64,
}

/// Parsed Excellon drill/rout data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExcellonLayer {
    /// Tools declared in the header.
    pub tools: Vec<DrillTool>,
    /// Drilled holes.
    pub holes: Vec<DrillHit>,
    /// Routed slots.
    pub slots: Vec<DrillSlot>,
    /// Units declared by the file.
    pub source_units: Option<crate::Unit>,
    /// Non-fatal parser and importer messages.
    pub warnings: Vec<String>,
}

impl ExcellonLayer {
    /// Bounding box of all holes and slots.
    pub fn bounds(&self) -> Option<Bounds> {
        let points = self
            .holes
            .iter()
            .map(|hit| hit.position)
            .chain(self.slots.iter().flat_map(|slot| [slot.start, slot.end]));
        Bounds::from_points(points).map(|bounds| {
            let radius = self
                .tools
                .iter()
                .map(|tool| tool.diameter_mm.abs() * 0.5)
                .fold(0.0, f64::max);
            bounds.expanded(radius)
        })
    }

    /// Number of drill hits.
    pub fn hole_count(&self) -> usize {
        self.holes.len()
    }

    /// Number of routed slots.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Diameter of a tool number.
    pub fn tool_diameter(&self, number: u32) -> Option<f64> {
        self.tools
            .iter()
            .find(|tool| tool.number == number)
            .map(|tool| tool.diameter_mm)
    }
}

/// Source geometry attached to a project layer.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerGeometry {
    /// A Gerber fabrication layer.
    Gerber(GerberLayer),
    /// An Excellon drill/rout layer.
    Excellon(ExcellonLayer),
}

impl LayerGeometry {
    /// Bounding box of the layer.
    pub fn bounds(&self) -> Option<Bounds> {
        match self {
            Self::Gerber(layer) => layer.bounds(),
            Self::Excellon(layer) => layer.bounds(),
        }
    }

    /// Number of primitives, holes and slots in the layer.
    pub fn object_count(&self) -> usize {
        match self {
            Self::Gerber(layer) => layer.object_count(),
            Self::Excellon(layer) => layer.hole_count() + layer.slot_count(),
        }
    }

    /// Non-fatal import warnings.
    pub fn warnings(&self) -> &[String] {
        match self {
            Self::Gerber(layer) => &layer.warnings,
            Self::Excellon(layer) => &layer.warnings,
        }
    }

    /// Source units declared by the file.
    pub fn source_units(&self) -> Option<crate::Unit> {
        match self {
            Self::Gerber(layer) => layer.source_units,
            Self::Excellon(layer) => layer.source_units,
        }
    }
}

fn circle_segments(radius: f64, tolerance: f64) -> u32 {
    if !radius.is_finite() || radius <= 0.0 {
        return 12;
    }
    let tolerance = tolerance.max(0.001);
    let count = (TAU / (2.0 * (1.0 - tolerance / radius).clamp(-1.0, 1.0).acos())).ceil() as u32;
    count.clamp(12, 128)
}

fn obround_outline(center: Point, width: f64, height: f64) -> Vec<Point> {
    let (half_w, half_h) = (width * 0.5, height * 0.5);
    let radius = half_w.min(half_h);
    let mut points = Vec::new();

    if (width - height).abs() < 1e-9 {
        let segments = circle_segments(radius, 0.01);
        for index in 0..segments {
            let angle = TAU * index as f64 / segments as f64;
            points.push(center + Point::new(angle.cos() * radius, angle.sin() * radius));
        }
        return points;
    }

    if width > height {
        let straight = half_w - radius;
        let right = Point::new(center.x + straight, center.y);
        let left = Point::new(center.x - straight, center.y);
        let segments = 16;
        for index in 0..=segments {
            let angle = -core::f64::consts::FRAC_PI_2
                + core::f64::consts::PI * index as f64 / segments as f64;
            points.push(right + Point::new(angle.cos() * radius, angle.sin() * radius));
        }
        for index in 0..=segments {
            let angle = core::f64::consts::FRAC_PI_2
                + core::f64::consts::PI * index as f64 / segments as f64;
            points.push(left + Point::new(angle.cos() * radius, angle.sin() * radius));
        }
    } else {
        let straight = half_h - radius;
        let top = Point::new(center.x, center.y + straight);
        let bottom = Point::new(center.x, center.y - straight);
        let segments = 16;
        for index in 0..=segments {
            let angle = core::f64::consts::PI * index as f64 / segments as f64;
            points.push(top + Point::new(angle.cos() * radius, angle.sin() * radius));
        }
        for index in 0..=segments {
            let angle =
                core::f64::consts::PI + core::f64::consts::PI * index as f64 / segments as f64;
            points.push(bottom + Point::new(angle.cos() * radius, angle.sin() * radius));
        }
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_from_points_and_expansion() {
        let bounds = Bounds::from_points([
            Point::new(2.0, 3.0),
            Point::new(-1.0, 5.0),
            Point::new(4.0, -2.0),
        ])
        .unwrap();
        assert_eq!(bounds.width(), 5.0);
        assert_eq!(bounds.height(), 7.0);
        assert_eq!(bounds.center(), Point::new(1.5, 1.5));
        assert!(bounds.expanded(1.0).contains(Point::new(-2.0, -3.0)));
    }

    #[test]
    fn closed_path_length_includes_last_segment() {
        let path = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(3.0, 0.0),
            Point::new(3.0, 4.0),
        ]);
        assert!((path.length() - 12.0).abs() < 1e-9);
    }

    #[test]
    fn circle_aperture_outline_is_round() {
        let points = Aperture::circle(10.0).outline(Point::new(5.0, 5.0), 0.0);
        assert!(points.len() >= 12);
        assert!(
            points
                .iter()
                .all(|point| (point.distance(Point::new(5.0, 5.0)) - 5.0).abs() < 0.1)
        );
    }
}
