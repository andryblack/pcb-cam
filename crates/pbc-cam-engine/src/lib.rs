//! Session and command engine for PBC-CAM.
//!
//! The engine owns a [`Project`](pbc_cam_core::Project), executes commands
//! through one JSON interface, and keeps an undo/redo history. The desktop UI
//! is a thin front end over this crate; future CLI and automation clients can
//! use the same command registry.

#![forbid(unsafe_code)]

mod command;
mod commands;
mod error;
mod session;

pub use command::{CommandInfo, CommandSpec, commands, find_command};
pub use error::EngineError;
pub use session::{MAX_UNDO, Session};

/// Version of the engine crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
