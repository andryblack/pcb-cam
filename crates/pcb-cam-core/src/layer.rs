//! Layer and source-format definitions.

use std::path::PathBuf;
use std::sync::Arc;

use std::collections::BTreeMap;

use crate::geometry::{Bounds, LayerGeometry, Primitive};
use crate::tool::ToolKind;

/// Stable identifier of a layer inside a [`Project`](crate::Project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    id: LayerId,
    name: String,
    format: SourceFormat,
    source: Option<PathBuf>,
    visible: bool,
    color: [u8; 3],
    geometry: Arc<LayerGeometry>,
    bounds: Option<Bounds>,
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
pub fn suggest_color(name: &str, format: SourceFormat, index: usize) -> [u8; 3] {
    let lower = name.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);

    if format == SourceFormat::Excellon || has("drill") {
        return [190, 203, 215];
    }
    if has("edge") || has("outline") || has("gm1") || has("gko") || has("profile") {
        return [238, 205, 112];
    }
    if has("mask") || has("gts") || has("gbs") {
        return [111, 211, 181];
    }
    if has("silk") || has("gto") || has("gbo") {
        return [220, 228, 238];
    }
    if has("paste") || has("gtp") || has("gbp") {
        return [166, 178, 190];
    }
    if has("f_cu") || has("gtl") || has("top") {
        return [227, 164, 94];
    }
    if has("b_cu") || has("gbl") || has("bottom") {
        return [196, 127, 69];
    }

    const PALETTE: [[u8; 3]; 8] = [
        [227, 164, 94],
        [196, 127, 69],
        [111, 211, 181],
        [220, 228, 238],
        [238, 205, 112],
        [151, 177, 213],
        [208, 139, 179],
        [190, 203, 215],
    ];
    PALETTE[index % PALETTE.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{GerberLayer, Primitive};

    fn empty_gerber() -> LayerGeometry {
        LayerGeometry::Gerber(GerberLayer::default())
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
            [227, 164, 94]
        );
        assert_eq!(
            suggest_color("board-Edge_Cuts.gbr", SourceFormat::Gerber, 0),
            [238, 205, 112]
        );
        assert_eq!(
            suggest_color("board.drl", SourceFormat::Excellon, 0),
            [190, 203, 215]
        );
    }
}
