//! CAM tasks in the project timeline.
//!
//! The public API is in transition from "operation" to "task"; the type
//! aliases at the end of this module let front ends use the newer name.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::geometry::{CopperGeometry, ExcellonLayer, GerberLayer, LayerGeometry, Path};
use crate::tool::{Tool, ToolKind};
use crate::{LayerId, Point, ToolId};

/// Stable identifier of a task inside a [`Project`](crate::Project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// Warnings for the selected source objects and machining tool.
    ///
    /// A drilling task only plunges vertically. Slots and holes that do not
    /// match the drill diameter must be handled by a hole-milling task.
    pub fn warnings(&self, layer: &ExcellonLayer, tool: &Tool) -> Vec<String> {
        let selected = |tool_number: u32| {
            self.source_tools.is_empty() || self.source_tools.contains(&tool_number)
        };
        let holes: Vec<_> = layer
            .holes
            .iter()
            .filter(|hole| selected(hole.tool))
            .collect();
        let slots: Vec<_> = layer
            .slots
            .iter()
            .filter(|slot| selected(slot.tool))
            .collect();
        if holes.is_empty() && slots.is_empty() {
            return vec!["No source holes or slots are selected.".to_owned()];
        }

        let mut warnings = Vec::new();
        if !slots.is_empty() {
            warnings.push(format!(
                "{} selected slot{} require a hole-milling task.",
                slots.len(),
                if slots.len() == 1 { "" } else { "s" },
            ));
        }
        if holes.is_empty() {
            warnings.push("No selected holes to drill.".to_owned());
            return warnings;
        }
        if tool.kind() != ToolKind::Drill {
            warnings.push(format!(
                "Drilling requires a drill bit; `{}` is {}.",
                tool.name(),
                tool.kind().label()
            ));
        }

        let smaller = holes
            .iter()
            .filter(|hole| hole.diameter_mm > tool.diameter_mm() + 1e-6)
            .count();
        let larger = holes
            .iter()
            .filter(|hole| hole.diameter_mm < tool.diameter_mm() - 1e-6)
            .count();
        if smaller > 0 {
            warnings.push(format!(
                "Drill Ø{:.3} mm is smaller than {smaller} selected hole{}; use a matching drill or hole milling.",
                tool.diameter_mm(),
                if smaller == 1 { "" } else { "s" },
            ));
        }
        if larger > 0 {
            warnings.push(format!(
                "Drill Ø{:.3} mm is larger than {larger} selected hole{}.",
                tool.diameter_mm(),
                if larger == 1 { "" } else { "s" },
            ));
        }
        warnings
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Parameters of a hole-milling task.
///
/// Unlike drilling, this task may move horizontally: holes larger than the
/// cutter are helically interpolated and slots are routed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoleMillOperation {
    /// Excellon layer that supplies the holes and slots.
    pub layer: LayerId,
    /// Machining tool from the user library.
    pub tool: ToolId,
    /// Source Excellon tool numbers selected for this task.
    pub source_tools: BTreeSet<u32>,
    /// Cut depth below the board surface, in millimetres.
    pub depth_mm: f64,
    /// Safe Z height, in millimetres.
    pub retract_mm: f64,
    /// XY feed rate.
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

impl HoleMillOperation {
    /// Sensible defaults for a new hole-milling task.
    pub fn new(layer: LayerId, tool: ToolId) -> Self {
        Self {
            layer,
            tool,
            source_tools: BTreeSet::new(),
            depth_mm: 1.8,
            retract_mm: 3.0,
            feed_mm_min: 300.0,
            plunge_mm_min: 120.0,
            dwell_s: 0.0,
            stepdown_mm: 0.5,
            spindle_rpm: 12_000,
        }
    }

    /// Warnings for the selected source objects and machining tool.
    pub fn warnings(&self, layer: &ExcellonLayer, tool: &Tool) -> Vec<String> {
        let selected = |tool_number: u32| {
            self.source_tools.is_empty() || self.source_tools.contains(&tool_number)
        };
        let holes: Vec<_> = layer
            .holes
            .iter()
            .filter(|hole| selected(hole.tool))
            .collect();
        let slots: Vec<_> = layer
            .slots
            .iter()
            .filter(|slot| selected(slot.tool))
            .collect();
        if holes.is_empty() && slots.is_empty() {
            return vec!["No source holes or slots are selected.".to_owned()];
        }

        let mut warnings = Vec::new();
        if !tool.kind().can_interpolate() {
            warnings.push(format!(
                "Hole milling requires an end mill or V-bit; `{}` is {}.",
                tool.name(),
                tool.kind().label()
            ));
        }
        let too_small_holes = holes
            .iter()
            .filter(|hole| hole.diameter_mm < tool.diameter_mm() - 1e-6)
            .count();
        let too_small_slots = slots
            .iter()
            .filter(|slot| slot.diameter_mm < tool.diameter_mm() - 1e-6)
            .count();
        if too_small_holes + too_small_slots > 0 {
            warnings.push(format!(
                "Tool Ø{:.3} mm is larger than {too_small_holes} hole{} and {too_small_slots} slot{}.",
                tool.diameter_mm(),
                if too_small_holes == 1 { "" } else { "s" },
                if too_small_slots == 1 { "" } else { "s" },
            ));
        }
        let wide_slots = slots
            .iter()
            .filter(|slot| slot.diameter_mm > tool.diameter_mm() + 1e-6)
            .count();
        if wide_slots > 0 {
            warnings.push(format!(
                "{wide_slots} selected slot{} wider than the tool; only the slot centreline is cut.",
                if wide_slots == 1 { "" } else { "s" },
            ));
        }
        warnings
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

/// Parameters of an edge cut (board outline or an internal cut-out).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutoutOperation {
    /// Gerber layer that supplies the closed contours.
    pub layer: LayerId,
    /// Machining tool from the user library.
    pub tool: ToolId,
    /// Which side of each contour to follow.
    pub side: ContourSide,
    /// Offset from the selected contour, in millimetres.
    pub offset_mm: f64,
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
    /// Sensible defaults for a new edge cut.
    pub fn new(layer: LayerId, tool: ToolId) -> Self {
        Self {
            layer,
            tool,
            side: ContourSide::Outside,
            offset_mm: 0.0,
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
        )?;
        if !self.offset_mm.is_finite() || self.offset_mm < 0.0 {
            return Err("edge cut offset must not be negative".into());
        }
        Ok(())
    }

    /// Offset all closed contours by the explicit edge-cut offset.
    pub fn toolpaths(&self, gerber: &GerberLayer) -> Vec<Path> {
        let distance = match self.side {
            ContourSide::Outside => self.offset_mm,
            ContourSide::Inside => -self.offset_mm,
            ContourSide::OnLine => 0.0,
        };
        gerber
            .closed_contours()
            .into_iter()
            .filter_map(|contour| crate::geometry::offset_closed_path(&contour, distance))
            .filter(|path| path.points.len() >= 2)
            .collect()
    }
}

/// Which side of a closed engraving contour the passes are placed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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

/// Parameters of a clearance (engraving) task.
///
/// Engraving follows the paths and polygon outlines found in a Gerber layer:
/// useful for silkscreen text, legends and shallow decorative fills.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Inward offset applied to the clearance boundary, in millimetres.
    pub outline_offset_mm: f64,
    /// Initial offset from the copper edge, in millimetres.
    pub offset_mm: f64,
    /// Number of offset passes for closed contours.
    pub passes: u32,
    /// Offset increase between passes, in millimetres.
    pub stepover_mm: f64,
    /// Spindle speed.
    pub spindle_rpm: u32,
    /// Optional layer whose largest closed contour clips the clearance paths.
    ///
    /// `None` uses the bounding box of visible layers. The boundary itself is
    /// never machined; it only limits the toolpaths.
    pub boundary_layer: Option<LayerId>,
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
            outline_offset_mm: 0.0,
            offset_mm: 0.0,
            passes: 1,
            stepover_mm: 0.5,
            spindle_rpm: 12_000,
            boundary_layer: None,
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
        if !self.outline_offset_mm.is_finite() || self.outline_offset_mm < 0.0 {
            return Err("clearance outline offset must not be negative".into());
        }
        if !self.offset_mm.is_finite() || self.offset_mm < 0.0 {
            return Err("clearance offset must not be negative".into());
        }
        if self.passes == 0 {
            return Err("clearance passes must be at least one".into());
        }
        if !self.stepover_mm.is_finite() || self.stepover_mm < 0.0 {
            return Err("clearance offset step must not be negative".into());
        }
        if self.passes > 1 && self.stepover_mm <= 0.0 {
            return Err(
                "clearance offset step must be greater than zero for multiple passes".into(),
            );
        }
        Ok(())
    }

    /// Generate isolation/clearance paths around the unioned copper.
    ///
    /// The source geometry is unioned first, then buffered by the tool radius
    /// plus the requested offset. Consequently internal overlaps (for example a
    /// trace ending at a pad centre) do not produce stray contours.
    pub fn toolpaths(&self, gerber: &GerberLayer, boundary: Option<&Path>) -> Vec<Path> {
        gerber.isolation_paths(
            self.outline_offset_mm,
            self.offset_mm,
            self.passes,
            self.stepover_mm,
            self.side.sign(),
            boundary,
        )
    }

    /// Generate clearance paths using a previously unioned copper snapshot.
    pub fn toolpaths_with_copper(
        &self,
        gerber: &GerberLayer,
        boundary: Option<&Path>,
        copper: Option<&CopperGeometry>,
    ) -> Vec<Path> {
        let Some(copper) = copper else {
            return self.toolpaths(gerber, boundary);
        };
        gerber
            .isolation_passes_with_copper(
                self.outline_offset_mm,
                self.offset_mm,
                self.passes,
                self.stepover_mm,
                self.side.sign(),
                boundary,
                copper,
            )
            .into_iter()
            .flatten()
            .collect()
    }

    /// Resolve the clipping boundary: selected layer contour or visible bounds.
    pub fn boundary_path(&self, project: &Project) -> Option<Path> {
        if let Some(layer_id) = self.boundary_layer
            && layer_id != self.layer
            && let Some(layer) = project.layer(layer_id)
            && let LayerGeometry::Gerber(gerber) = layer.geometry()
            && let Some(path) = gerber.closed_contours().into_iter().max_by(|a, b| {
                let area = |path: &Path| {
                    path.bounds()
                        .map(|bounds| bounds.width() * bounds.height())
                        .unwrap_or(0.0)
                };
                area(a).total_cmp(&area(b))
            })
        {
            return Some(path);
        }
        let bounds = project.bounds(true)?;
        Some(Path::closed(vec![
            Point::new(bounds.min.x, bounds.min.y),
            Point::new(bounds.max.x, bounds.min.y),
            Point::new(bounds.max.x, bounds.max.y),
            Point::new(bounds.min.x, bounds.max.y),
        ]))
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OperationKind {
    /// Drill all holes and rout all slots in an Excellon layer.
    Drill(DrillOperation),
    /// Spot-drill the selected hole centres without a diameter.
    SpotDrill(SpotDrillOperation),
    /// Mill holes and slots with lateral/helical motion.
    HoleMill(HoleMillOperation),
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
            Self::HoleMill(_) => "Hole milling",
            Self::Cutout(_) => "Edge cut",
            Self::Engrave(_) => "Clearance",
        }
    }

    /// Machining tool used by the task.
    pub const fn tool(&self) -> ToolId {
        match self {
            Self::Drill(task) => task.tool,
            Self::SpotDrill(task) => task.tool,
            Self::HoleMill(task) => task.tool,
            Self::Cutout(task) => task.tool,
            Self::Engrave(task) => task.tool,
        }
    }

    /// Source layer consumed by the task.
    pub const fn source_layer(&self) -> LayerId {
        match self {
            Self::Drill(task) => task.layer,
            Self::SpotDrill(task) => task.layer,
            Self::HoleMill(task) => task.layer,
            Self::Cutout(task) => task.layer,
            Self::Engrave(task) => task.layer,
        }
    }

    /// Validate the task parameters.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Drill(task) => task.validate(),
            Self::SpotDrill(task) => task.validate(),
            Self::HoleMill(task) => task.validate(),
            Self::Cutout(task) => task.validate(),
            Self::Engrave(task) => task.validate(),
        }
    }
}

/// A single CAM task in the project timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
/// Newer name for a hole-milling task.
pub type HoleMillTask = HoleMillOperation;
/// Newer name for an edge cut task.
pub type CutoutTask = CutoutOperation;
/// Edge-cut task alias.
pub type EdgeCutOperation = CutoutOperation;
/// Edge-cut task alias.
pub type EdgeCutTask = CutoutOperation;
/// Newer name for an engraving task.
pub type EngraveTask = EngraveOperation;
/// User-facing name for an engraving/clearance task.
pub type ClearanceOperation = EngraveOperation;
/// User-facing alias for a clearance task.
pub type ClearanceTask = EngraveOperation;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Aperture, DrillHit, DrillTool, ExcellonLayer, GerberLayer, Primitive};
    use crate::tool::{ToolKind, ToolLibrary};
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
    fn drill_warnings_are_derived_from_selected_holes_and_tool() {
        let layer = ExcellonLayer {
            tools: vec![DrillTool {
                number: 1,
                diameter_mm: 0.31,
            }],
            holes: vec![DrillHit {
                position: Point::new(0.0, 0.0),
                tool: 1,
                diameter_mm: 0.31,
            }],
            slots: Vec::new(),
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut library = ToolLibrary::new();
        let mut drill = DrillOperation::new(LayerId::new(1), ToolId::new(1));
        drill.source_tools.insert(1);

        let small_drill = library.closest(ToolKind::Drill, 0.3).unwrap();
        drill.tool = small_drill;
        let warnings = drill.warnings(&layer, library.tool(small_drill).unwrap());
        assert!(warnings.iter().any(|warning| warning.contains("smaller")));

        let large_drill = library.closest(ToolKind::Drill, 1.0).unwrap();
        drill.tool = large_drill;
        let warnings = drill.warnings(&layer, library.tool(large_drill).unwrap());
        assert!(warnings.iter().any(|warning| warning.contains("larger")));

        let end_mill = library.add(
            "End mill 0.30 mm",
            ToolKind::EndMill,
            0.3,
            200.0,
            80.0,
            12_000,
        );
        drill.tool = end_mill;
        let warnings = drill.warnings(&layer, library.tool(end_mill).unwrap());
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("requires a drill"))
        );

        let mut mill = HoleMillOperation::new(LayerId::new(1), end_mill);
        mill.source_tools.insert(1);
        let warnings = mill.warnings(&layer, library.tool(end_mill).unwrap());
        assert!(warnings.is_empty());
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
    fn clearance_offset_passes_expand_a_filled_copper_shape() {
        let square = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(0.0, 5.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Region {
                contours: vec![square],
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.side = EngraveSide::Outside;
        clearance.offset_mm = 0.1;
        clearance.passes = 3;
        clearance.stepover_mm = 0.2;
        let paths = clearance.toolpaths(&gerber, None);
        assert_eq!(paths.len(), 3);
        let first = paths[0].bounds().unwrap();
        let last = paths[2].bounds().unwrap();
        assert!(last.width() > first.width());
        assert!(last.height() > first.height());
        assert!((first.width() - 10.2).abs() < 0.05);
    }

    #[test]
    fn clearance_around_a_trace_and_pad_is_one_union_contour() {
        let trace = Path::new(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
        let gerber = GerberLayer {
            primitives: vec![
                Primitive::Stroke {
                    path: trace,
                    aperture: Aperture::circle(0.5),
                    polarity: Polarity::Dark,
                },
                Primitive::Flash {
                    position: Point::new(10.0, 0.0),
                    aperture: Aperture::circle(1.5),
                    rotation_degrees: 0.0,
                    polarity: Polarity::Dark,
                },
            ],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.offset_mm = 0.1;
        let paths = clearance.toolpaths(&gerber, None);
        // A trace ending in a pad must not leave a separate circular contour
        // inside the pad. The union has one exterior at this offset.
        assert_eq!(paths.len(), 1);
        assert!(paths[0].closed);
    }

    #[test]
    fn clearance_around_an_open_trace_closes_with_round_ends() {
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
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.side = EngraveSide::Outside;
        clearance.offset_mm = 0.1;
        clearance.passes = 2;
        clearance.stepover_mm = 0.5;
        let paths = clearance.toolpaths(&gerber, None);
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| path.closed));
        let first = paths[0].bounds().unwrap();
        let second = paths[1].bounds().unwrap();
        assert!((first.width() - 10.4).abs() < 0.05);
        assert!((first.height() - 0.4).abs() < 0.05);
        assert!((second.width() - 11.4).abs() < 0.05);
        assert!((second.height() - 1.4).abs() < 0.05);
    }

    #[test]
    fn clearance_boundary_passes_are_offset_by_the_step() {
        let copper = Path::closed(vec![
            Point::new(2.0, 2.0),
            Point::new(3.0, 2.0),
            Point::new(3.0, 3.0),
            Point::new(2.0, 3.0),
        ]);
        let boundary = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(0.0, 5.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Region {
                contours: vec![copper],
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.outline_offset_mm = 0.1;
        clearance.offset_mm = 0.1;
        clearance.passes = 2;
        clearance.stepover_mm = 0.2;
        let paths = clearance.toolpaths(&gerber, Some(&boundary));
        let widths: Vec<f64> = paths
            .iter()
            .filter_map(Path::bounds)
            .map(|bounds| bounds.width())
            .collect();
        assert!(widths.iter().any(|width| (width - 1.2).abs() < 0.05));
        assert!(widths.iter().any(|width| (width - 1.6).abs() < 0.05));
    }

    #[test]
    fn clearance_adds_island_contours_inside_holes() {
        let outer = Path::closed(vec![
            Point::new(2.0, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.0, 8.0),
        ]);
        let inner = Path::closed(vec![
            Point::new(3.0, 3.0),
            Point::new(7.0, 3.0),
            Point::new(7.0, 7.0),
            Point::new(3.0, 7.0),
        ]);
        let boundary = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Region {
                contours: vec![outer, inner],
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.outline_offset_mm = 0.0;
        clearance.offset_mm = 0.1;
        let paths = clearance.toolpaths(&gerber, Some(&boundary));
        assert!(paths.iter().any(|path| {
            path.bounds()
                .is_some_and(|b| (b.width() - 3.8).abs() < 0.05 && (b.height() - 3.8).abs() < 0.05)
        }));
    }

    #[test]
    fn clearance_boundary_is_not_a_toolpath() {
        let copper = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]);
        let boundary = Path::closed(vec![
            Point::new(-1.0, -1.0),
            Point::new(11.0, -1.0),
            Point::new(11.0, 11.0),
            Point::new(-1.0, 11.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Region {
                contours: vec![copper],
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.outline_offset_mm = 0.0;
        clearance.offset_mm = 0.1;
        let paths = clearance.toolpaths(&gerber, Some(&boundary));
        assert!(!paths.is_empty());
        let bounds = paths[0].bounds().unwrap();
        assert!((bounds.min.x + 0.1).abs() < 0.05);
        assert!((bounds.max.x - 10.1).abs() < 0.05);
    }

    #[test]
    fn clearance_boundary_can_be_the_source_layer_bbox() {
        let mut project = Project::new("Boundary");
        let layer_id = project.add_layer(
            "Copper",
            None,
            None,
            LayerGeometry::Gerber(GerberLayer {
                primitives: vec![Primitive::Stroke {
                    path: Path::new(vec![Point::new(1.0, 1.0), Point::new(5.0, 1.0)]),
                    aperture: Aperture::circle(0.2),
                    polarity: Polarity::Dark,
                }],
                source_units: Some(crate::Unit::Millimeter),
                warnings: Vec::new(),
            }),
        );
        let mut clearance = EngraveOperation::new(layer_id, ToolId::new(1));
        clearance.boundary_layer = Some(layer_id);
        let boundary = clearance.boundary_path(&project).unwrap();
        let bounds = boundary.bounds().unwrap();
        assert!((bounds.width() - 4.2).abs() < 1e-6);
        assert!((bounds.height() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn clearance_boundary_clips_paths_without_machining_the_boundary() {
        let copper = Path::closed(vec![
            Point::new(2.0, 2.0),
            Point::new(4.0, 2.0),
            Point::new(4.0, 4.0),
            Point::new(2.0, 4.0),
        ]);
        let boundary = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Region {
                contours: vec![copper],
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.offset_mm = 0.1;
        let paths = clearance.toolpaths(&gerber, Some(&boundary));
        assert_eq!(paths.len(), 1);
        let bounds = paths[0].bounds().unwrap();
        assert!((bounds.width() - 2.2).abs() < 0.05);
        assert!((bounds.height() - 2.2).abs() < 0.05);
    }

    #[test]
    fn clearance_boundary_clips_the_copper_source() {
        let copper = Path::closed(vec![
            Point::new(2.0, 2.0),
            Point::new(4.0, 2.0),
            Point::new(4.0, 4.0),
            Point::new(2.0, 4.0),
        ]);
        let boundary = Path::closed(vec![
            Point::new(2.5, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(2.5, 10.0),
        ]);
        let gerber = GerberLayer {
            primitives: vec![Primitive::Region {
                contours: vec![copper],
                polarity: Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        };
        let mut clearance = EngraveOperation::new(LayerId::new(1), ToolId::new(1));
        clearance.offset_mm = 0.1;
        let paths = clearance.toolpaths(&gerber, Some(&boundary));
        assert!(!paths.is_empty());
        assert!(paths.iter().all(|path| path.closed));
        assert!(
            paths
                .iter()
                .flat_map(|path| &path.points)
                .any(|point| point.x < 2.5)
        );
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
        assert_eq!(operation.kind_label(), "Edge cut");
    }
}
