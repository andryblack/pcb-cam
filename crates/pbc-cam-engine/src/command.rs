//! Command registry metadata.
//!
//! Every user-visible action in PBC-CAM should be a command. The same
//! registry will later drive menus, the command palette, automation and a
//! headless CLI.

use serde::Serialize;
use serde_json::Value;

use crate::Session;

/// A command implementation.
pub type Run = fn(&mut Session, &Value) -> Result<Value, String>;
/// A command availability predicate.
pub type Enabled = fn(&Session) -> Result<(), String>;

/// Metadata and implementation of one command.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// Stable dotted command id.
    pub id: &'static str,
    /// Short label for menus and search.
    pub label: &'static str,
    /// One-line explanation.
    pub description: &'static str,
    /// Keyboard shortcut, if any.
    pub shortcut: Option<&'static str>,
    /// Whether a successful run creates an undo step.
    pub undoable: bool,
    /// Implementation.
    pub run: Run,
    /// Optional availability check.
    pub enabled: Option<Enabled>,
}

impl CommandSpec {
    /// Check whether the command can run now.
    pub fn is_enabled(&self, session: &Session) -> bool {
        self.enabled.is_none_or(|enabled| enabled(session).is_ok())
    }

    /// Why the command is unavailable, if it is.
    pub fn disabled_reason(&self, session: &Session) -> Option<String> {
        self.enabled.and_then(|enabled| enabled(session).err())
    }

    /// Public metadata for a front end or automation client.
    pub fn info(&self, session: &Session) -> CommandInfo {
        let disabled_reason = self.disabled_reason(session);
        CommandInfo {
            id: self.id,
            label: self.label,
            description: self.description,
            shortcut: self.shortcut,
            undoable: self.undoable,
            enabled: disabled_reason.is_none(),
            disabled_reason,
        }
    }
}

/// Serializable command metadata.
#[derive(Debug, Clone, Serialize)]
pub struct CommandInfo {
    /// Command id.
    pub id: &'static str,
    /// Display label.
    pub label: &'static str,
    /// Description.
    pub description: &'static str,
    /// Shortcut text.
    pub shortcut: Option<&'static str>,
    /// Whether the command is undoable.
    pub undoable: bool,
    /// Whether it can run in the current session.
    pub enabled: bool,
    /// Why it cannot run, when disabled.
    pub disabled_reason: Option<String>,
}

pub(crate) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "project.new",
        label: "New project",
        description: "Replace the current project with an empty one.",
        shortcut: Some("Cmd/Ctrl+N"),
        undoable: false,
        run: crate::commands::project_new,
        enabled: None,
    },
    CommandSpec {
        id: "file.import_gerber",
        label: "Import Gerber",
        description: "Import a Gerber RS-274X fabrication layer.",
        shortcut: None,
        undoable: true,
        run: crate::commands::import_gerber,
        enabled: None,
    },
    CommandSpec {
        id: "file.import_excellon",
        label: "Import Excellon",
        description: "Import an Excellon drill and rout layer.",
        shortcut: None,
        undoable: true,
        run: crate::commands::import_excellon,
        enabled: None,
    },
    CommandSpec {
        id: "file.export_gcode",
        label: "Export G-code",
        description: "Post-process an enabled operation into a CNC program.",
        shortcut: None,
        undoable: false,
        run: crate::commands::export_gcode,
        enabled: Some(crate::commands::export_gcode_enabled),
    },
    CommandSpec {
        id: "layer.set_visible",
        label: "Show/hide layer",
        description: "Toggle the visibility of a source layer.",
        shortcut: None,
        undoable: true,
        run: crate::commands::layer_set_visible,
        enabled: None,
    },
    CommandSpec {
        id: "layer.rename",
        label: "Rename layer",
        description: "Change a source layer's display name.",
        shortcut: None,
        undoable: true,
        run: crate::commands::layer_rename,
        enabled: None,
    },
    CommandSpec {
        id: "layer.remove",
        label: "Remove layer",
        description: "Remove a source layer and dependent operations.",
        shortcut: None,
        undoable: true,
        run: crate::commands::layer_remove,
        enabled: None,
    },
    CommandSpec {
        id: "units.set",
        label: "Set display units",
        description: "Switch the project display units.",
        shortcut: None,
        undoable: true,
        run: crate::commands::units_set,
        enabled: None,
    },
    CommandSpec {
        id: "tool.create",
        label: "Add tool",
        description: "Add a cutting or drilling tool to the tool table.",
        shortcut: None,
        undoable: true,
        run: crate::commands::tool_create,
        enabled: None,
    },
    CommandSpec {
        id: "tool.update",
        label: "Update tool",
        description: "Change a tool's diameter, feeds or spindle speed.",
        shortcut: None,
        undoable: true,
        run: crate::commands::tool_update,
        enabled: None,
    },
    CommandSpec {
        id: "tool.remove",
        label: "Remove tool",
        description: "Remove a tool from the tool table.",
        shortcut: None,
        undoable: true,
        run: crate::commands::tool_remove,
        enabled: None,
    },
    CommandSpec {
        id: "operation.add_drill",
        label: "Add drilling operation",
        description: "Create a drill operation for an Excellon layer.",
        shortcut: None,
        undoable: true,
        run: crate::commands::operation_add_drill,
        enabled: Some(crate::commands::add_drill_enabled),
    },
    CommandSpec {
        id: "operation.update_drill",
        label: "Update drilling operation",
        description: "Change the depth, feeds or spindle speed of a drill operation.",
        shortcut: None,
        undoable: true,
        run: crate::commands::operation_update_drill,
        enabled: None,
    },
    CommandSpec {
        id: "operation.remove",
        label: "Remove operation",
        description: "Remove an operation from the timeline.",
        shortcut: None,
        undoable: true,
        run: crate::commands::operation_remove,
        enabled: None,
    },
    CommandSpec {
        id: "operation.set_enabled",
        label: "Enable/disable operation",
        description: "Include or skip an operation during post-processing.",
        shortcut: None,
        undoable: true,
        run: crate::commands::operation_set_enabled,
        enabled: None,
    },
    CommandSpec {
        id: "operation.move",
        label: "Reorder operation",
        description: "Move an operation to another slot in the timeline.",
        shortcut: None,
        undoable: true,
        run: crate::commands::operation_move,
        enabled: None,
    },
    CommandSpec {
        id: "edit.undo",
        label: "Undo",
        description: "Undo the last project change.",
        shortcut: Some("Cmd/Ctrl+Z"),
        undoable: false,
        run: crate::commands::undo,
        enabled: None,
    },
    CommandSpec {
        id: "edit.redo",
        label: "Redo",
        description: "Redo the last undone project change.",
        shortcut: Some("Cmd/Ctrl+Shift+Z"),
        undoable: false,
        run: crate::commands::redo,
        enabled: None,
    },
];

/// Find a command by its stable id.
pub fn find_command(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|command| command.id == id)
}

/// All registered commands.
pub fn commands() -> &'static [CommandSpec] {
    COMMANDS
}
