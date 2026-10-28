//! Core domain model for PBC-CAM.
//!
//! This crate is GUI-independent. It will grow to contain the Gerber and
//! Excellon parsers, geometry primitives and toolpath generation. The desktop
//! application currently uses it for project, layer and unit state only.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod layer;
mod project;
mod units;

pub use layer::{Layer, LayerId, SourceFormat};
pub use project::{Project, UNTITLED_PROJECT};
pub use units::Unit;

/// Version of the core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
