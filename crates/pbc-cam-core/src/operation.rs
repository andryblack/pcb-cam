//! CAM tasks in the project timeline.
//!
//! The public API is in transition from "operation" to "task"; the type
//! aliases at the end of this module let front ends use the newer name.

use crate::LayerId;

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
    /// Spindle speed.
    pub spindle_rpm: u32,
}

impl DrillOperation {
    /// Sensible defaults for a new drilling task.
    pub const fn new(layer: LayerId) -> Self {
        Self {
            layer,
            depth_mm: 1.8,
            retract_mm: 3.0,
            feed_mm_min: 300.0,
            plunge_mm_min: 120.0,
            dwell_s: 0.1,
            spindle_rpm: 10_000,
        }
    }

    /// Validate the task before generating G-code.
    pub fn validate(&self) -> Result<(), String> {
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
    /// Cutter diameter in millimetres.
    pub tool_diameter_mm: f64,
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
    pub const fn new(layer: LayerId) -> Self {
        Self {
            layer,
            tool_diameter_mm: 1.0,
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
        validate_tool(self.tool_diameter_mm)?;
        validate_z_and_feeds(
            self.depth_mm,
            self.retract_mm,
            self.feed_mm_min,
            self.plunge_mm_min,
            self.spindle_rpm,
        )
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
    /// Engraving cutter diameter in millimetres.
    pub tool_diameter_mm: f64,
    /// Engraving depth below the surface, in millimetres.
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

impl EngraveOperation {
    /// Sensible defaults for a new engraving task.
    pub const fn new(layer: LayerId) -> Self {
        Self {
            layer,
            tool_diameter_mm: 0.3,
            depth_mm: 0.2,
            retract_mm: 2.0,
            feed_mm_min: 400.0,
            plunge_mm_min: 100.0,
            spindle_rpm: 12_000,
        }
    }

    /// Validate the task before generating G-code.
    pub fn validate(&self) -> Result<(), String> {
        validate_tool(self.tool_diameter_mm)?;
        validate_z_and_feeds(
            self.depth_mm,
            self.retract_mm,
            self.feed_mm_min,
            self.plunge_mm_min,
            self.spindle_rpm,
        )
    }
}

fn validate_tool(diameter: f64) -> Result<(), String> {
    if !diameter.is_finite() || diameter <= 0.0 {
        return Err("tool diameter must be greater than zero".into());
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
            Self::Cutout(_) => "Contour cut",
            Self::Engrave(_) => "Engrave",
        }
    }

    /// Source layer consumed by the task.
    pub const fn source_layer(&self) -> LayerId {
        match self {
            Self::Drill(task) => task.layer,
            Self::Cutout(task) => task.layer,
            Self::Engrave(task) => task.layer,
        }
    }

    /// Validate the task parameters.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Drill(task) => task.validate(),
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
/// Newer name for a contour cut task.
pub type CutoutTask = CutoutOperation;
/// Newer name for an engraving task.
pub type EngraveTask = EngraveOperation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drill_defaults_validate() {
        let drill = DrillOperation::new(LayerId::new(1));
        assert!(drill.validate().is_ok());
    }

    #[test]
    fn drill_validation_rejects_bad_values() {
        let mut drill = DrillOperation::new(LayerId::new(1));
        drill.depth_mm = 0.0;
        assert!(drill.validate().is_err());
        drill.depth_mm = 1.0;
        drill.spindle_rpm = 0;
        assert!(drill.validate().is_err());
    }

    #[test]
    fn cutout_and_engrave_validate_tools() {
        let mut cutout = CutoutOperation::new(LayerId::new(1));
        assert!(cutout.validate().is_ok());
        cutout.tool_diameter_mm = 0.0;
        assert!(cutout.validate().is_err());

        let mut engrave = EngraveOperation::new(LayerId::new(1));
        assert!(engrave.validate().is_ok());
        engrave.depth_mm = -1.0;
        assert!(engrave.validate().is_err());
    }

    #[test]
    fn task_aliases_report_source_layer() {
        let layer = LayerId::new(7);
        let operation = Operation::new(
            OperationId::new(1),
            "Cut".to_owned(),
            OperationKind::Cutout(CutoutOperation::new(layer)),
        );
        assert_eq!(operation.source_layer(), layer);
        assert_eq!(operation.kind_label(), "Contour cut");
    }
}
