//! Core domain model for PBC-CAM.
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

pub use layer::{Layer, LayerId, SourceFormat, suggest_color};
pub use operation::{DrillOperation, Operation, OperationId, OperationKind};
pub use project::{Project, UNTITLED_PROJECT};
pub use tool::{Tool, ToolId, ToolKind};
pub use units::Unit;

pub use formats::{ParseError, parse_excellon, parse_gerber};
pub use gcode::{GcodeProgram, generate_drill_gcode};
pub use geometry::{
    Aperture, ApertureHole, ApertureShape, Bounds, DrillHit, DrillSlot, DrillTool, ExcellonLayer,
    GerberLayer, LayerGeometry, Path, Point, Polarity, Primitive,
};

/// Version of the core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
