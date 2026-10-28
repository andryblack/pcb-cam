//! CAM tasks in the project timeline.
//!
//! The public API is in transition from "operation" to "task"; the type
//! aliases at the end of this module let front ends use the newer name.

use std::collections::BTreeSet;

use crate::geometry::{GerberLayer, Path, offset_closed_path, offset_open_path};
use crate::{LayerId, ToolId};

/// Stable identifier of a task inside a [`Project`](crate::Project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(u64);

impl OperationId {
    /// Construct an identifier from its numeric value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Numeric value of the identifier.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Which side of a closed contour the cutter follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContourSide {
    /// Keep the original part size and cut outside the contour.
    #[default]
    Outside,
    /// Cut inside the contour.
    Inside,
    /// Cut directly on the contour.
    OnLine,
}

impl ContourSide {
    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Outside => "Outside",
            Self::Inside => "Inside",
            Self::OnLine => "On line",
        }
    }

    /// JSON value used by commands.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Inside => "inside",
            Self::OnLine => "on_line",
        }
    }
}

/// Parameters of a drilling task.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillOperation {
    /// Excellon layer that supplies the holes.
    pub layer: LayerId,
    /// Machining tool from the user library.
    pub tool: ToolId,
    /// Source Excellon tool numbers selected for this task.
    ///
    /// An empty set means all source tools.
    pub source_tools: BTreeSet<u32>,
    /// Cut depth below the board surface, in millimetres.
    pub depth_mm: f64,
    /// Safe Z height, in millimetres.
    pub retract_mm: f64,
    /// XY feed rate for slots.
    pub feed_mm_min: f64,
    /// Z plunge rate.
    pub plunge_mm_min: f64,
    /// Dwell at the bottom of each hole, in seconds.
    pub dwell_s: f64,
    /// Z step-down for helical interpolation, in millimetres.
    pub stepdown_mm: f64,
    /// Spindle speed.
    pub spindle_rpm: u32,
}

impl DrillOperation {
    /// Sensible defaults for a new drilling task.
    pub fn new(layer: LayerId, tool: ToolId) -> Self {
        Self {
            layer,
            tool,
            source_tools: BTreeSet::new(),
            depth_mm: 1.8,
            retract_mm: 3.0,
            feed_mm_min: 300.0,
            plunge_mm_min: 120.0,
            dwell_s: 0.1,
            stepdown_mm: 0.5,
            spindle_rpm: 10_000,
        }
    }

    /// Validate the task before generating G-code.
    pub fn validate(&self) -> Result<(), String> {
        validate_tool_id(self.tool)?;
        validate_z_and_feeds(
            self.depth_mm,
            self.retract_mm,
            self.feed_mm_min,
            self.plunge_mm_min,
            self.spindle_rpm,
        )?;
        if !self.dwell_s.is_finite() || self.dwell_s < 0.0 {
            return Err("dwell must not be negative".into());
        }
        if !self.stepdown_mm.is_finite() || self.stepdown_mm <= 0.0 {
            return Err("helical step-down must be greater than zero".into());
        }
        Ok(())
    }
}

/// Parameters of a spot-drilling (centre-punching) task.
///
/// Source holes select positions only; the hole diameter is irrelevant. The
/// default machining tool is a cone/spot drill.
#[derive(Debug, Clone, PartialEq)]
pub struct SpotDrillOperation {
    /// Excellon layer that supplies the hole positions.
    pub layer: LayerId,
    /// Machining tool from the user library.
    pub tool: ToolId,
    /// Source Excellon tool numbers selected for this task.
    ///
    /// An empty set means all source tools with holes.
    pub source_tools: BTreeSet<u32>,
    /// Spot depth below the board surface, in millimetres.
    pub depth_mm: f64,
    /// Safe Z height, in millimetres.
    pub retract_mm: f64,
    /// XY feed rate.
    pub feed_mm_min: f64,
    /// Z plunge rate.
    pub plunge_mm_min: f64,
    /// Dwell at the bottom of each spot, in seconds.
    pub dwell_s: f64,
    /// Spindle speed.
    pub spindle_rpm: u32,
}

impl SpotDrillOperation {
    /// Sensible defaults for a new spot-drilling task.
    pub fn new(layer: LayerId, tool: ToolId) -> Self {
        Self {
            layer,
            tool,
            source_tools: BTreeSet::new(),
            depth_mm: 0.5,
            retract_mm: 3.0,
            feed_mm_min: 250.0,
            plunge_mm_min: 80.0,
            dwell_s: 0.05,
            spindle_rpm: 15_000,
        }
    }

    /// Validate the task before generating G-code.
    pub fn validate(&self) -> Result<(), String> {
        validate_tool_id(self.tool)?;
        validate_z_and_feeds(
            self.depth_mm,
            self.retract_mm,
            self.feed_mm_min,
            self.plunge_mm_min,
            self.spindle_rpm,
        )?;
        if !self.dwell_s.is_finite() || self.dwell_s < 0.0 {
            return Err("dwell must not be negative".into());
        }
        Ok(())
    }
}

/// Parameters of a contour cut (board outline or an internal cut-out).
#[derive(Debug, Clone, PartialEq)]
pub struct CutoutOperation {
    /// Gerber layer that supplies the closed contours.
    pub layer: LayerId,
    /// Machining tool from the user library.
    pub tool: ToolId,
    /// Which side of each contour to follow.
    pub side: ContourSide,
    /// Cut depth below the board surface, in millimetres.
    pub depth_mm: f64,
    /// Safe Z height, in millimetres.
    pub retract_mm: f64,
    /// XY feed rate.
    pub feed_mm_min: f64,
    /// Z plunge rate.
    pub plunge_mm_min: f64,
    /// Spindle speed.
    pub spindle_rpm: u32,
}

impl CutoutOperation {
    /// Sensible defaults for a new contour cut.
    pub fn new(layer: LayerId, tool: ToolId) -> Self {
        Self {
            layer,
            tool,
            side: ContourSide::Outside,
            depth_mm: 1.8,
            retract_mm: 3.0,
            feed_mm_min: 300.0,
            plunge_mm_min: 120.0,
            spindle_rpm: 10_000,
        }
    }

    /// Validate the task before generating G-code.
    pub fn validate(&self) -> Result<(), String> {
        validate_tool_id(self.tool)?;
        validate_z_and_feeds(
            self.depth_mm,
            self.retract_mm,
            self.feed_mm_min,
            self.plunge_mm_min,
            self.spindle_rpm,
        )
    }
}

/// Which side of a closed engraving contour the passes are placed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngraveSide {
    /// Offset passes move outward from the contour.
    #[default]
    Outside,
    /// Offset passes move inward into the contour.
    Inside,
}

impl EngraveSide {
    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Outside => "Outside",
            Self::Inside => "Inside",
        }
    }

    /// JSON value used by commands.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Inside => "inside",
        }
    }

    /// Signed direction of an offset pass.
    pub const fn sign(self) -> f64 {
        match self {
            Self::Outside => 1.0,
            Self::Inside => -1.0,
        }
    }
}

/// Parameters of an engraving task.
///
/// Engraving follows the paths and polygon outlines found in a Gerber layer:
/// useful for silkscreen text, legends and shallow decorative fills.
#[derive(Debug, Clone, PartialEq)]
pub struct EngraveOperation {
    /// Gerber layer that supplies the paths.
    pub layer: LayerId,
    /// Machining tool from the user library.
    pub tool: ToolId,
    /// Engraving depth below the surface, in millimetres.
    pub depth_mm: f64,
    /// Safe Z height, in millimetres.
    pub retract_mm: f64,
    /// XY feed rate.
    pub feed_mm_min: f64,
    /// Z plunge rate.
    pub plunge_mm_min: f64,
    /// Side of closed contours for multi-pass engraving.
    pub side: EngraveSide,
    /// Initial offset from the source contour, in millimetres.
    pub offset_mm: f64,
    /// Number of offset passes for closed contours.
    pub passes: u32,
    /// Offset increase between passes, in millimetres.
    pub stepover_mm: f64,
    /// Spindle speed.
    pub spindle_rpm: u32,
}

impl EngraveOperation {
    /// Sensible defaults for a new engraving task.
    pub fn new(layer: LayerId, tool: ToolId) -> Self {
        Self {
            layer,
            tool,
            depth_mm: 0.2,
            retract_mm: 2.0,
            feed_mm_min: 400.0,
            plunge_mm_min: 100.0,
            side: EngraveSide::Outside,
            offset_mm: 0.0,
            passes: 1,
            stepover_mm: 0.5,
            spindle_rpm: 12_000,
        }
    }

    /// Validate the task before generating G-code.
    pub fn validate(&self) -> Result<(), String> {
        validate_tool_id(self.tool)?;
        validate_z_and_feeds(
            self.depth_mm,
            self.retract_mm,
            self.feed_mm_min,
            self.plunge_mm_min,
            self.spindle_rpm,
        )?;
        if !self.offset_mm.is_finite() || self.offset_mm < 0.0 {
            return Err("engraving offset must not be negative".into());
        }
        if self.passes == 0 {
            return Err("engraving passes must be at least one".into());
        }
        if !self.stepover_mm.is_finite() || self.stepover_mm < 0.0 {
            return Err("engraving offset step must not be negative".into());
        }
        if self.passes > 1 && self.stepover_mm <= 0.0 {
            return Err(
                "engraving offset step must be greater than zero for multiple passes".into(),
            );
        }
        Ok(())
    }

    /// Generate engraved paths, including the requested offset passes.
    ///
    /// Closed contours are offset outside/inside their enclosed area. Open
    /// paths are offset left for Outside and right for Inside, so traces can be
    /// engraved beside the centreline as well as on it.
    pub fn toolpaths(&self, gerber: &GerberLayer) -> Vec<Path> {
        let mut result = Vec::new();
        for path in gerber.engraving_paths() {
            for pass in 0..self.passes {
                let distance =
                    self.side.sign() * (self.offset_mm + f64::from(pass) * self.stepover_mm);
                if distance.abs() < 1e-9 {
                    result.push(path.clone());
                    continue;
                }
                let offset = if path.closed {
                    offset_closed_path(&path, distance)
                } else {
                    offset_open_path(&path, distance)
                };
                if let Some(offset) = offset {
                    result.push(offset);
                }
            }
        }
        result
    }
}

fn validate_tool_id(tool: ToolId) -> Result<(), String> {
    if tool.value() == 0 {
        return Err("task has no machining tool".into());
    }
    Ok(())
}

fn validate_z_and_feeds(
    depth: f64,
    retract: f64,
    feed: f64,
    plunge: f64,
    spindle: u32,
) -> Result<(), String> {
    if !depth.is_finite() || depth <= 0.0 {
        return Err("cut depth must be greater than zero".into());
    }
    if !retract.is_finite() || retract <= 0.0 {
        return Err("retract height must be greater than zero".into());
    }
    if !feed.is_finite() || feed <= 0.0 {
        return Err("feed rate must be greater than zero".into());
    }
    if !plunge.is_finite() || plunge <= 0.0 {
        return Err("plunge rate must be greater than zero".into());
    }
    if spindle == 0 {
        return Err("spindle speed must be greater than zero".into());
    }
    Ok(())
}

/// The kind-specific parameters of a task.
#[derive(Debug, Clone, PartialEq)]
pub enum OperationKind {
    /// Drill all holes and rout all slots in an Excellon layer.
    Drill(DrillOperation),
    /// Spot-drill the selected hole centres without a diameter.
    SpotDrill(SpotDrillOperation),
    /// Cut along closed Gerber contours.
    Cutout(CutoutOperation),
    /// Engrave paths and polygon outlines.
    Engrave(EngraveOperation),
}

impl OperationKind {
    /// Short type label.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Drill(_) => "Drill",
            Self::SpotDrill(_) => "Spot drill",
            Self::Cutout(_) => "Contour cut",
            Self::Engrave(_) => "Engrave",
        }
    }

    /// Machining tool used by the task.
    pub const fn tool(&self) -> ToolId {
        match self {
            Self::Drill(task) => task.tool,
            Self::SpotDrill(task) => task.tool,
            Self::Cutout(task) => task.tool,
            Self::Engrave(task) => task.tool,
        }
    }

    /// Source layer consumed by the task.
    pub const fn source_layer(&self) -> LayerId {
        match self {
            Self::Drill(task) => task.layer,
            Self::SpotDrill(task) => task.layer,
            Self::Cutout(task) => task.layer,
            Self::Engrave(task) => task.layer,
        }
    }

    /// Validate the task parameters.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Drill(task) => task.validate(),
            Self::SpotDrill(task) => task.validate(),
            Self::Cutout(task) => task.validate(),
            Self::Engrave(task) => task.validate(),
        }
    }
}

/// A single CAM task in the project timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    id: OperationId,
    name: String,
    enabled: bool,
    kind: OperationKind,
}

impl Operation {
    pub(crate) fn new(id: OperationId, name: String, kind: OperationKind) -> Self {
        Self {
            id,
            name,
            enabled: true,
            kind,
        }
    }

    /// Stable task identifier.
    pub const fn id(&self) -> OperationId {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the task. Empty names are ignored.
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        let name = name.trim();
        if !name.is_empty() {
            self.name = name.to_owned();
        }
    }

    /// Whether the task is included in combined G-code.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable the task.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Task parameters.
    pub const fn kind(&self) -> &OperationKind {
        &self.kind
    }

    /// Mutable task parameters.
    pub fn kind_mut(&mut self) -> &mut OperationKind {
        &mut self.kind
    }

    /// Short type label.
    pub const fn kind_label(&self) -> &'static str {
        self.kind.label()
    }

    /// Source layer this task consumes.
    pub const fn source_layer(&self) -> LayerId {
        self.kind.source_layer()
    }

    /// Machining tool referenced by this task.
    pub const fn tool(&self) -> ToolId {
        self.kind.tool()
    }

    /// Validate the task parameters.
    pub fn validate(&self) -> Result<(), String> {
        self.kind.validate()
    }
}

/// Newer name for an operation in the project task list.
pub type Task = Operation;
/// Newer name for a task identifier.
pub type TaskId = OperationId;
/// Newer name for the task parameter enum.
pub type TaskKind = OperationKind;
/// Newer name for a drilling task.
pub type DrillTask = DrillOperation;
/// Newer name for a spot-drilling task.
pub type SpotDrillTask = SpotDrillOperation;
/// Newer name for a contour cut task.
pub type CutoutTask = CutoutOperation;
/// Newer name for an engraving task.
pub type EngraveTask = EngraveOperation;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Aperture, GerberLayer, Primitive};
    use crate::{Point, Polarity};

    #[test]
    fn drill_defaults_validate() {
        let drill = DrillOperation::new(LayerId::new(1), ToolId::new(1));
        assert!(drill.validate().is_ok());
    }

    #[test]
    fn drill_validation_rejects_bad_values() {
        let mut drill = DrillOperation::new(LayerId::new(1), ToolId::new(1));
        drill.depth_mm = 0.0;
        assert!(drill.validate().is_err());
        drill.depth_mm = 1.0;
        drill.spindle_rpm = 0;
        assert!(drill.validate().is_err());
    }

    #[test]
    fn spot_drill_defaults_validate() {
        let mut spot = SpotDrillOperation::new(LayerId::new(1), ToolId::new(1));
        assert!(spot.validate().is_ok());
        spot.depth_mm = 0.0;
        assert!(spot.validate().is_err());
        spot.depth_mm = 0.5;
        spot.dwell_s = -1.0;
        assert!(spot.validate().is_err());
    }

    #[test]
    fn cutout_and_engrave_validate_tools() {
        let mut cutout = CutoutOperation::new(LayerId::new(1), ToolId::new(2));
        assert!(cutout.validate().is_ok());
        cutout.tool = ToolId::new(0);
        assert!(cutout.validate().is_err());

        let mut engrave = EngraveOperation::new(LayerId::new(1), ToolId::new(3));
        assert!(engrave.validate().is_ok());
        engrave.depth_mm = -1.0;
        assert!(engrave.validate().is_err());
        engrave.depth_mm = 0.2;
        engrave.passes = 0;
        assert!(engrave.validate().is_err());
        engrave.passes = 2;
        engrave.stepover_mm = 0.0;
        assert!(engrave.validate().is_err());
    }

    #[test]
    fn engrave_offset_passes_expand_closed_contours() {
        let path = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(0.0, 5.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Stroke {
                path,
                aperture: Aperture::circle(0.2),
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut engrave = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        engrave.side = EngraveSide::Outside;
        engrave.offset_mm = 0.1;
        engrave.passes = 3;
        engrave.stepover_mm = 0.2;
        let paths = engrave.toolpaths(&gerber);
        assert_eq!(paths.len(), 3);
        let first = paths[0].bounds().unwrap();
        let last = paths[2].bounds().unwrap();
        assert!(last.width() > first.width());
        assert!(last.height() > first.height());
    }

    #[test]
    fn engrave_open_paths_are_offset_to_the_selected_side() {
        let path = Path::new(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Stroke {
                path,
                aperture: Aperture::circle(0.2),
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut engrave = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        engrave.side = EngraveSide::Outside;
        engrave.offset_mm = 1.0;
        engrave.passes = 2;
        engrave.stepover_mm = 0.5;
        let paths = engrave.toolpaths(&gerber);
        assert_eq!(paths.len(), 2);
        assert!((paths[0].points[0].y - 1.0).abs() < 1e-9);
        assert!((paths[1].points[0].y - 1.5).abs() < 1e-9);

        engrave.side = EngraveSide::Inside;
        let paths = engrave.toolpaths(&gerber);
        assert_eq!(paths.len(), 2);
        assert!((paths[0].points[0].y + 1.0).abs() < 1e-9);
    }

    #[test]
    fn task_aliases_report_source_layer() {
        let layer = LayerId::new(7);
        let operation = Operation::new(
            OperationId::new(1),
            "Cut".to_owned(),
            OperationKind::Cutout(CutoutOperation::new(layer, ToolId::new(2))),
        );
        assert_eq!(operation.source_layer(), layer);
        assert_eq!(operation.kind_label(), "Contour cut");
    }
}
