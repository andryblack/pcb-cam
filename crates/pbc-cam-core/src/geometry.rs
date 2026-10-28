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

    /// Human-readable shape summary for source-tool lists.
    pub fn label(&self) -> String {
        match self.shape {
            ApertureShape::Circle { diameter } => format!("Ø{diameter:.3} mm"),
            ApertureShape::Rectangle { width, height } => {
                format!("{width:.2} × {height:.2} mm")
            }
            ApertureShape::Obround { width, height } => {
                format!("obround {width:.2} × {height:.2} mm")
            }
            ApertureShape::Polygon {
                diameter, vertices, ..
            } => format!("polygon {vertices} × Ø{diameter:.2} mm"),
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

    /// Closed contours suitable for a contour cut.
    ///
    /// Only closed strokes, paths whose last point meets their first, and
    /// region outlines are returned. Flashes and open paths are ignored.
    pub fn closed_contours(&self) -> Vec<Path> {
        let mut contours = Vec::new();
        for primitive in &self.primitives {
            match primitive {
                Primitive::Stroke { path, .. } => {
                    if let Some(path) = normalized_contour(path, true) {
                        contours.push(path);
                    }
                }
                Primitive::Region {
                    contours: region, ..
                } => {
                    for contour in region {
                        if let Some(path) = normalized_contour(contour, true) {
                            contours.push(path);
                        }
                    }
                }
                Primitive::Flash { .. } => {}
            }
        }
        contours
    }

    /// Whether any closed contour is available without cloning the geometry.
    pub fn has_closed_contours(&self) -> bool {
        self.primitives.iter().any(|primitive| match primitive {
            Primitive::Stroke { path, .. } => normalized_contour(path, true).is_some(),
            Primitive::Region { contours, .. } => contours
                .iter()
                .any(|contour| normalized_contour(contour, true).is_some()),
            Primitive::Flash { .. } => false,
        })
    }

    /// Whether any engraving path is available without cloning the geometry.
    pub fn has_engraving_paths(&self) -> bool {
        self.primitives.iter().any(|primitive| match primitive {
            Primitive::Stroke { path, .. } => normalized_contour(path, false).is_some(),
            Primitive::Region { contours, .. } => contours
                .iter()
                .any(|contour| normalized_contour(contour, false).is_some()),
            Primitive::Flash { .. } => true,
        })
    }

    /// All paths suitable for engraving: open strokes, closed contours,
    /// region outlines and flash outlines.
    pub fn engraving_paths(&self) -> Vec<Path> {
        let mut paths = Vec::new();
        for primitive in &self.primitives {
            match primitive {
                Primitive::Stroke { path, .. } => {
                    if let Some(path) = normalized_contour(path, false) {
                        paths.push(path);
                    }
                }
                Primitive::Region { contours, .. } => {
                    for contour in contours {
                        if let Some(path) = normalized_contour(contour, false) {
                            paths.push(path);
                        }
                    }
                }
                Primitive::Flash {
                    position,
                    aperture,
                    rotation_degrees,
                    ..
                } => {
                    let points = aperture.outline(*position, *rotation_degrees);
                    if points.len() >= 3 {
                        paths.push(Path::closed(points));
                    }
                }
            }
        }
        paths
    }
}

/// Offset a closed polygon.
///
/// Positive `distance` moves the contour outwards, negative inwards, relative
/// to the polygon orientation. Joins are mitred with a bounded miter length.
/// Self-intersections in strongly concave contours are not resolved; this is
/// sufficient for simple board outlines and a first CAM implementation.
pub fn offset_closed_path(path: &Path, distance: f64) -> Option<Path> {
    if !distance.is_finite() {
        return None;
    }
    let mut points = path.points.clone();
    if points.len() >= 2
        && let (Some(first), Some(last)) = (points.first(), points.last())
        && first.distance(*last) < 1e-7
    {
        points.pop();
    }
    if points.len() < 3 || distance.abs() < 1e-9 {
        return Some(Path::closed(points));
    }

    let area = signed_area(&points);
    if area.abs() < 1e-12 {
        return None;
    }
    let outward_sign = if area > 0.0 { 1.0 } else { -1.0 };
    let count = points.len();
    let mut result = Vec::with_capacity(count);

    for index in 0..count {
        let current = points[index];
        let previous = points[(index + count - 1) % count];
        let next = points[(index + 1) % count];
        let incoming = current - previous;
        let outgoing = next - current;
        if incoming.distance(Point::ORIGIN) < 1e-9 || outgoing.distance(Point::ORIGIN) < 1e-9 {
            result.push(current);
            continue;
        }

        let incoming_normal = normalize(Point::new(
            incoming.y * outward_sign,
            -incoming.x * outward_sign,
        ));
        let outgoing_normal = normalize(Point::new(
            outgoing.y * outward_sign,
            -outgoing.x * outward_sign,
        ));
        let mut miter = Point::new(
            incoming_normal.x + outgoing_normal.x,
            incoming_normal.y + outgoing_normal.y,
        );
        if miter.distance(Point::ORIGIN) < 1e-9 {
            miter = incoming_normal;
        } else {
            miter = normalize(miter);
        }
        let denominator = miter.x * incoming_normal.x + miter.y * incoming_normal.y;
        let scale = if denominator.abs() < 0.2 {
            4.0 * distance
        } else {
            distance / denominator
        };
        let limit = 4.0 * distance.abs();
        let scale = scale.clamp(-limit, limit);
        result.push(Point::new(
            current.x + miter.x * scale,
            current.y + miter.y * scale,
        ));
    }

    Some(Path::closed(result))
}

/// Offset an open polyline to the left by a signed distance.
///
/// Positive distances move left of the path direction, negative distances move
/// right. Joins are mitred with a bounded miter length. Used by engraving
/// passes on traces and other open Gerber paths.
pub fn offset_open_path(path: &Path, distance: f64) -> Option<Path> {
    if !distance.is_finite() {
        return None;
    }
    let mut points = Vec::with_capacity(path.points.len());
    for point in &path.points {
        if points
            .last()
            .is_none_or(|last: &Point| last.distance(*point) > 1e-9)
        {
            points.push(*point);
        }
    }
    if points.len() < 2 {
        return None;
    }
    if distance.abs() < 1e-9 {
        return Some(Path::new(points));
    }

    let count = points.len();
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let current = points[index];
        let incoming = if index > 0 {
            let delta = current - points[index - 1];
            if delta.distance(Point::ORIGIN) > 1e-9 {
                Some(normalize(Point::new(-delta.y, delta.x)))
            } else {
                None
            }
        } else {
            None
        };
        let outgoing = if index + 1 < count {
            let delta = points[index + 1] - current;
            if delta.distance(Point::ORIGIN) > 1e-9 {
                Some(normalize(Point::new(-delta.y, delta.x)))
            } else {
                None
            }
        } else {
            None
        };

        let normal = match (incoming, outgoing) {
            (Some(incoming), Some(outgoing)) => {
                let mut miter = Point::new(incoming.x + outgoing.x, incoming.y + outgoing.y);
                if miter.distance(Point::ORIGIN) < 1e-9 {
                    incoming
                } else {
                    miter = normalize(miter);
                    let denominator = miter.x * incoming.x + miter.y * incoming.y;
                    let scale = if denominator.abs() < 0.2 {
                        4.0 * distance
                    } else {
                        distance / denominator
                    };
                    let limit = 4.0 * distance.abs();
                    let scale = scale.clamp(-limit, limit);
                    result.push(Point::new(
                        current.x + miter.x * scale,
                        current.y + miter.y * scale,
                    ));
                    continue;
                }
            }
            (Some(normal), None) | (None, Some(normal)) => normal,
            (None, None) => continue,
        };
        result.push(Point::new(
            current.x + normal.x * distance,
            current.y + normal.y * distance,
        ));
    }

    (result.len() >= 2).then(|| Path::new(result))
}

fn normalized_contour(path: &Path, closed_required: bool) -> Option<Path> {
    if path.points.len() < 2 {
        return None;
    }
    let mut points = path.points.clone();
    let endpoints_meet = points.first().is_some_and(|first| {
        points
            .last()
            .is_some_and(|last| first.distance(*last) < 1e-7)
    });
    let closed = path.closed || endpoints_meet;
    if closed
        && let (Some(first), Some(last)) = (points.first(), points.last())
        && first.distance(*last) < 1e-7
    {
        points.pop();
    }
    if points.len() < 2 {
        return None;
    }
    if closed && points.len() < 3 {
        return None;
    }
    if closed_required && !closed {
        return None;
    }
    Some(Path { points, closed })
}

fn signed_area(points: &[Point]) -> f64 {
    let mut sum = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

fn normalize(point: Point) -> Point {
    let length = point.x.hypot(point.y);
    if length > 0.0 {
        Point::new(point.x / length, point.y / length)
    } else {
        Point::ORIGIN
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
