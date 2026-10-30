//! Layer and source-format definitions.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use std::collections::BTreeMap;

use crate::geometry::{Bounds, LayerGeometry, Point, Primitive};
use crate::tool::ToolKind;

/// Stable identifier of a layer inside a [`Project`](crate::Project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LayerId(u64);

impl LayerId {
    /// Construct an identifier from its numeric value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Numeric value of the identifier.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// File format a layer was imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceFormat {
    /// Gerber RS-274X / X2 fabrication data.
    Gerber,
    /// Excellon drill and rout data.
    Excellon,
}

impl SourceFormat {
    /// Short label suitable for menus and layer rows.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gerber => "Gerber",
            Self::Excellon => "Excellon",
        }
    }

    /// Lower-case file extensions commonly used by this format.
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Gerber => &[
                "gbr", "ger", "gtl", "gbl", "gts", "gbs", "gto", "gbo", "gko", "gdl", "gm1", "gtp",
                "gbp", "gba", "gta",
            ],
            Self::Excellon => &["drl", "xln", "exc", "nc", "txt"],
        }
    }

    /// Guess a format from a path extension.
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        if Self::Gerber.extensions().contains(&extension.as_str()) {
            Some(Self::Gerber)
        } else if Self::Excellon.extensions().contains(&extension.as_str()) {
            Some(Self::Excellon)
        } else {
            None
        }
    }
}

/// A single source layer in a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    id: LayerId,
    name: String,
    format: SourceFormat,
    source: Option<PathBuf>,
    visible: bool,
    color: [u8; 3],
    geometry: Arc<LayerGeometry>,
    bounds: Option<Bounds>,
    /// Whether the layer is mirrored left/right.
    #[serde(default)]
    flip_h: bool,
    /// Whether the layer is mirrored top/bottom.
    #[serde(default)]
    flip_v: bool,
}

impl Layer {
    pub(crate) fn new(
        id: LayerId,
        name: String,
        source: Option<PathBuf>,
        color: [u8; 3],
        geometry: LayerGeometry,
    ) -> Self {
        let format = match geometry {
            LayerGeometry::Gerber(_) => SourceFormat::Gerber,
            LayerGeometry::Excellon(_) => SourceFormat::Excellon,
        };
        let bounds = geometry.bounds();
        Self {
            id,
            name,
            format,
            source,
            visible: true,
            color,
            geometry: Arc::new(geometry),
            bounds,
            flip_h: false,
            flip_v: false,
        }
    }

    /// Stable layer identifier.
    pub const fn id(&self) -> LayerId {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the layer. Empty names are ignored.
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        let name = name.trim();
        if !name.is_empty() {
            self.name = name.to_owned();
        }
    }

    /// Format this layer was imported from.
    pub const fn format(&self) -> SourceFormat {
        self.format
    }

    /// Path of the imported source file, when the layer came from disk.
    pub fn source(&self) -> Option<&std::path::Path> {
        self.source.as_deref()
    }

    /// Set or clear the source path.
    pub fn set_source(&mut self, source: Option<PathBuf>) {
        self.source = source;
    }

    /// Whether the layer is visible on the canvas.
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// Show or hide the layer.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// RGB tint used to draw the layer.
    pub const fn color(&self) -> [u8; 3] {
        self.color
    }

    /// Change the layer tint.
    pub fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
    }

    /// Parsed geometry.
    pub fn geometry(&self) -> &LayerGeometry {
        &self.geometry
    }

    /// Whether the layer is currently mirrored left/right.
    pub const fn flip_h(&self) -> bool {
        self.flip_h
    }

    /// Whether the layer is currently mirrored top/bottom.
    pub const fn flip_v(&self) -> bool {
        self.flip_v
    }

    /// Mirror the layer geometry around `pivot`, refresh its bounds and toggle
    /// the matching flip flag.
    pub fn flip(&mut self, horizontal: bool, vertical: bool, pivot: Point) {
        if !horizontal && !vertical {
            return;
        }
        let geometry = Arc::make_mut(&mut self.geometry);
        geometry.flip(horizontal, vertical, pivot);
        self.bounds = geometry.bounds();
        self.flip_h ^= horizontal;
        self.flip_v ^= vertical;
    }

    /// Move an already flipped layer to a new pair of mirror axes.
    ///
    /// The current geometry is unflipped around `old_pivot` and flipped around
    /// `new_pivot` using the same flags, so only the axis position changes.
    pub fn rebase_flips(&mut self, old_pivot: Point, new_pivot: Point) {
        if (!self.flip_h && !self.flip_v) || old_pivot == new_pivot {
            return;
        }
        let geometry = Arc::make_mut(&mut self.geometry);
        geometry.flip(self.flip_h, self.flip_v, old_pivot);
        geometry.flip(self.flip_h, self.flip_v, new_pivot);
        self.bounds = geometry.bounds();
    }

    /// Clone this layer's geometry and derive a shadow copy in `reference`'s
    /// mirrored frame.
    ///
    /// The transform is computed from the two layer flip flags: the boundary's
    /// own flips are cancelled and the reference layer's flips are applied.
    /// This is the shadow copy used for a clearance boundary; the boundary
    /// layer itself stays unchanged.
    pub fn shadow_geometry(&self, reference: &Layer, pivot: Point) -> LayerGeometry {
        let horizontal = self.flip_h ^ reference.flip_h;
        let vertical = self.flip_v ^ reference.flip_v;
        let mut geometry = (*self.geometry).clone();
        if horizontal || vertical {
            geometry.flip(horizontal, vertical, pivot);
        }
        geometry
    }

    /// Whether this layer looks like the board outline, used to choose a stable
    /// mirror centre when flipping individual bottom-side layers.
    pub(crate) fn is_board_outline(&self) -> bool {
        if self.format != SourceFormat::Gerber {
            return false;
        }
        let name = self.name.to_ascii_lowercase();
        ["edge", "outline", "gm1", "gko", "profile"]
            .iter()
            .any(|needle| name.contains(needle))
    }

    /// Cached bounding box in millimetres.
    pub const fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// Number of drawing primitives, drills and slots.
    pub fn object_count(&self) -> usize {
        self.geometry.object_count()
    }

    /// Non-fatal import warnings.
    pub fn warnings(&self) -> &[String] {
        self.geometry.warnings()
    }
}

/// A read-only tool/aperture found in a source fabrication file.
///
/// These identify objects for task filtering. They are deliberately separate
/// from the user's machining tool library.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceToolInfo {
    /// Excellon tool number, when applicable.
    pub number: Option<u32>,
    /// Nominal diameter, when known.
    pub diameter_mm: Option<f64>,
    /// Human-readable shape.
    pub label: String,
    /// Number of source objects made with this tool.
    pub count: usize,
    /// Machining tool type suggested when creating a library tool from this entry.
    pub suggested_kind: ToolKind,
}

impl Layer {
    /// Read-only tools/apertures present in this source layer.
    pub fn source_tools(&self) -> Vec<SourceToolInfo> {
        match self.geometry.as_ref() {
            LayerGeometry::Excellon(excellon) => {
                let mut tools: Vec<SourceToolInfo> = excellon
                    .tools
                    .iter()
                    .map(|tool| {
                        let holes = excellon
                            .holes
                            .iter()
                            .filter(|hole| hole.tool == tool.number)
                            .count();
                        let slots = excellon
                            .slots
                            .iter()
                            .filter(|slot| slot.tool == tool.number)
                            .count();
                        SourceToolInfo {
                            number: Some(tool.number),
                            diameter_mm: Some(tool.diameter_mm),
                            label: format!("T{} · Ø{:.3} mm", tool.number, tool.diameter_mm),
                            count: holes + slots,
                            suggested_kind: if holes > 0 || slots == 0 {
                                ToolKind::Drill
                            } else {
                                ToolKind::EndMill
                            },
                        }
                    })
                    .collect();
                for hole in &excellon.holes {
                    if !tools.iter().any(|tool| tool.number == Some(hole.tool)) {
                        tools.push(SourceToolInfo {
                            number: Some(hole.tool),
                            diameter_mm: Some(hole.diameter_mm),
                            label: format!("T{} · Ø{:.3} mm", hole.tool, hole.diameter_mm),
                            count: 1,
                            suggested_kind: ToolKind::Drill,
                        });
                    }
                }
                tools
            }
            LayerGeometry::Gerber(gerber) => {
                let mut groups: BTreeMap<String, (Option<f64>, usize)> = BTreeMap::new();
                for primitive in &gerber.primitives {
                    let aperture = match primitive {
                        Primitive::Stroke { aperture, .. } | Primitive::Flash { aperture, .. } => {
                            aperture
                        }
                        Primitive::Region { .. } => continue,
                    };
                    let label = aperture.label();
                    let diameter = match aperture.shape {
                        crate::geometry::ApertureShape::Circle { diameter } => Some(diameter),
                        _ => None,
                    };
                    groups
                        .entry(label)
                        .and_modify(|(_, count)| *count += 1)
                        .or_insert((diameter, 1));
                }
                groups
                    .into_iter()
                    .map(|(label, (diameter, count))| SourceToolInfo {
                        number: None,
                        diameter_mm: diameter,
                        label,
                        count,
                        suggested_kind: ToolKind::EndMill,
                    })
                    .collect()
            }
        }
    }
}

/// Pick a sensible layer colour from its name and position.
pub fn suggest_color(_name: &str, format: SourceFormat, _index: usize) -> [u8; 3] {
    match format {
        SourceFormat::Gerber => [204, 245, 142],
        SourceFormat::Excellon => [190, 203, 215],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{GerberLayer, Primitive};

    fn empty_gerber() -> LayerGeometry {
        LayerGeometry::Gerber(GerberLayer::default())
    }

    fn flash_layer(id: u64, name: &str, x: f64, y: f64) -> Layer {
        let mut gerber = GerberLayer::default();
        gerber.primitives.push(Primitive::Flash {
            position: Point::new(x, y),
            aperture: crate::geometry::Aperture::circle(1.0),
            rotation_degrees: 0.0,
            polarity: crate::geometry::Polarity::Dark,
        });
        Layer::new(
            LayerId::new(id),
            name.to_owned(),
            None,
            [1, 2, 3],
            LayerGeometry::Gerber(gerber),
        )
    }

    fn flash_position(geometry: &LayerGeometry) -> Point {
        let LayerGeometry::Gerber(gerber) = geometry else {
            panic!("expected Gerber geometry");
        };
        let Primitive::Flash { position, .. } = &gerber.primitives[0] else {
            panic!("expected flash");
        };
        *position
    }

    #[test]
    fn shadow_geometry_follows_the_reference_layer_flips() {
        let pivot = Point::new(2.0, 2.0);
        let mut source = flash_layer(1, "Source", 1.0, 1.0);
        source.flip(true, false, pivot);

        let mut boundary = flash_layer(2, "Boundary", 1.0, 1.0);
        boundary.flip(false, true, pivot);
        assert_eq!(flash_position(boundary.geometry()), Point::new(1.0, 3.0));

        let shadow = boundary.shadow_geometry(&source, pivot);
        assert_eq!(flash_position(&shadow), Point::new(3.0, 1.0));

        let mut already_flipped = flash_layer(3, "Boundary 2", 1.0, 1.0);
        already_flipped.flip(true, false, pivot);
        let shadow = already_flipped.shadow_geometry(&source, pivot);
        assert_eq!(flash_position(&shadow), Point::new(3.0, 1.0));
    }

    #[test]
    fn flip_flags_survive_serialization() {
        let pivot = Point::new(2.0, 2.0);
        let mut source = flash_layer(1, "Source", 1.0, 1.0);
        source.flip(true, false, pivot);
        assert!(source.flip_h());
        assert!(!source.flip_v());

        let text = serde_json::to_string(&source).unwrap();
        let loaded: Layer = serde_json::from_str(&text).unwrap();
        assert!(loaded.flip_h());
        assert!(!loaded.flip_v());
        let boundary = flash_layer(2, "Boundary", 1.0, 1.0);
        let shadow = boundary.shadow_geometry(&loaded, pivot);
        assert_eq!(flash_position(&shadow), Point::new(3.0, 1.0));

        let mut toggled = loaded;
        toggled.flip(true, false, pivot);
        assert!(!toggled.flip_h());
        assert_eq!(flash_position(toggled.geometry()), Point::new(1.0, 1.0));
    }

    #[test]
    fn layer_derives_format_and_bounds() {
        let mut gerber = GerberLayer::default();
        gerber.primitives.push(Primitive::Flash {
            position: crate::geometry::Point::new(10.0, 20.0),
            aperture: crate::geometry::Aperture::circle(2.0),
            rotation_degrees: 0.0,
            polarity: crate::geometry::Polarity::Dark,
        });
        let layer = Layer::new(
            LayerId::new(1),
            "Top".to_owned(),
            None,
            [1, 2, 3],
            LayerGeometry::Gerber(gerber),
        );
        assert_eq!(layer.format(), SourceFormat::Gerber);
        assert_eq!(layer.object_count(), 1);
        assert_eq!(layer.bounds().unwrap().width(), 2.0);
    }

    #[test]
    fn source_tools_suggest_drill_or_end_mill() {
        use crate::geometry::{DrillHit, DrillSlot, DrillTool, ExcellonLayer};
        let layer = Layer::new(
            LayerId::new(3),
            "Drills".to_owned(),
            None,
            [1, 2, 3],
            LayerGeometry::Excellon(ExcellonLayer {
                tools: vec![
                    DrillTool {
                        number: 1,
                        diameter_mm: 0.8,
                    },
                    DrillTool {
                        number: 2,
                        diameter_mm: 1.6,
                    },
                ],
                holes: vec![DrillHit {
                    position: crate::geometry::Point::new(1.0, 1.0),
                    tool: 1,
                    diameter_mm: 0.8,
                }],
                slots: vec![DrillSlot {
                    start: crate::geometry::Point::new(2.0, 2.0),
                    end: crate::geometry::Point::new(3.0, 2.0),
                    tool: 2,
                    diameter_mm: 1.6,
                }],
                source_units: Some(crate::Unit::Millimeter),
                warnings: Vec::new(),
            }),
        );
        let tools = layer.source_tools();
        assert_eq!(tools[0].suggested_kind, ToolKind::Drill);
        assert_eq!(tools[1].suggested_kind, ToolKind::EndMill);
    }

    #[test]
    fn visibility_and_name_are_editable() {
        let mut layer = Layer::new(
            LayerId::new(2),
            "Drill".to_owned(),
            None,
            [1, 2, 3],
            empty_gerber(),
        );
        layer.set_visible(false);
        layer.set_name("  Drills  ");
        assert!(!layer.visible());
        assert_eq!(layer.name(), "Drills");
        layer.set_name("   ");
        assert_eq!(layer.name(), "Drills");
    }

    #[test]
    fn colors_recognise_common_roles() {
        assert_eq!(
            suggest_color("board-F_Cu.gbr", SourceFormat::Gerber, 0),
            [204, 245, 142]
        );
        assert_eq!(
            suggest_color("board-Edge_Cuts.gbr", SourceFormat::Gerber, 0),
            [204, 245, 142]
        );
        assert_eq!(
            suggest_color("board.drl", SourceFormat::Excellon, 0),
            [190, 203, 215]
        );
    }
}
