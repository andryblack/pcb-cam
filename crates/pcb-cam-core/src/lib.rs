//! Core domain model for PCB-CAM.
//!
//! This crate is GUI-independent. It will grow to contain the Gerber and
//! Excellon parsers, geometry primitives and toolpath generation. The desktop
//! application currently uses it for project, layer and unit state only.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod formats;
pub mod gcode;
pub mod geometry;
pub mod operation;
pub mod tool;

mod layer;
mod project;
mod units;

pub use geometry::GerberRegion;
pub use layer::{Layer, LayerId, SourceFormat, SourceToolInfo, suggest_color};
pub use operation::{
    ClearanceOperation, ClearanceTask, ContourSide, CutoutOperation, CutoutTask, DrillOperation,
    DrillTask, EdgeCutOperation, EdgeCutTask, EngraveOperation, EngraveSide, EngraveTask,
    HoleMillOperation, HoleMillTask, Operation, OperationId, OperationKind, SpotDrillOperation,
    SpotDrillTask, Task, TaskId, TaskKind,
};
pub use project::{Project, UNTITLED_PROJECT};
pub use tool::{Tool, ToolId, ToolKind, ToolLibrary};
pub use units::Unit;

pub use formats::{ParseError, parse_excellon, parse_gerber};
pub use gcode::{
    GcodeProgram, generate_all_gcode, generate_drill_gcode, generate_task_gcode,
    generate_tasks_gcode,
};
pub use geometry::{
    Aperture, ApertureHole, ApertureShape, Bounds, DrillHit, DrillSlot, DrillTool, ExcellonLayer,
    GerberLayer, LayerGeometry, Path, Point, Polarity, Primitive, offset_closed_path,
};

/// Version of the core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
