//! Project session, command execution and undo history.

use std::sync::Arc;

use pcb_cam_core::{Project, ToolLibrary};
use serde_json::Value;

use crate::command::find_command;
use crate::error::EngineError;

/// Maximum number of undo steps kept in memory.
pub const MAX_UNDO: usize = 128;

/// An open project and its command history.
#[derive(Debug)]
pub struct Session {
    project: Arc<Project>,
    undo: Vec<Arc<Project>>,
    redo: Vec<Arc<Project>>,
    dirty: bool,
    tool_library: ToolLibrary,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Start with an empty untitled project.
    pub fn new() -> Self {
        Self {
            project: Arc::new(Project::default()),
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            tool_library: ToolLibrary::new(),
        }
    }

    /// Start with an existing project.
    pub fn from_project(project: Project) -> Self {
        Self {
            project: Arc::new(project),
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            tool_library: ToolLibrary::new(),
        }
    }

    /// User machining tool library, shared by all projects.
    pub fn tool_library(&self) -> &ToolLibrary {
        &self.tool_library
    }

    /// Mutable machining tool library.
    pub fn tool_library_mut(&mut self) -> &mut ToolLibrary {
        &mut self.tool_library
    }

    /// Replace the machining tool library (for persisted settings).
    pub fn set_tool_library(&mut self, mut library: ToolLibrary) {
        library.normalize();
        self.tool_library = library;
    }

    /// The current project.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The current project as a shared snapshot.
    pub fn project_arc(&self) -> Arc<Project> {
        Arc::clone(&self.project)
    }

    /// Execute a command by its stable id.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value, EngineError> {
        let command = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_owned()))?;

        if let Some(enabled) = command.enabled
            && let Err(reason) = enabled(self)
        {
            return Err(EngineError::Disabled {
                id: id.to_owned(),
                reason,
            });
        }

        if !command.undoable {
            return (command.run)(self, params).map_err(EngineError::Command);
        }

        let before = Arc::clone(&self.project);
        let dirty_before = self.dirty;
        match (command.run)(self, params) {
            Ok(value) => {
                if !Arc::ptr_eq(&before, &self.project) {
                    self.undo.push(before);
                    if self.undo.len() > MAX_UNDO {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                    self.dirty = true;
                }
                Ok(value)
            }
            Err(message) => {
                self.project = before;
                self.dirty = dirty_before;
                Err(EngineError::Command(message))
            }
        }
    }

    /// Undo the last change. Returns `false` when the history is empty.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(Arc::clone(&self.project));
        self.project = previous;
        self.dirty = true;
        true
    }

    /// Redo the last undone change. Returns `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(Arc::clone(&self.project));
        self.project = next;
        self.dirty = true;
        true
    }

    /// Whether an undo step is available.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether a redo step is available.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of undo steps in memory.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps in memory.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Whether the project has unsaved changes since the last reset.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Replace the project and clear history (used by `project.new`).
    pub(crate) fn replace_project(&mut self, project: Project) {
        self.project = Arc::new(project);
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
    }

    /// Mutable project access for command implementations.
    ///
    /// The immutable `Project` clone keeps undo snapshots cheap: only the
    /// vectors of layers and tasks are copied, while imported
    /// geometry stays shared through `Arc`.
    pub(crate) fn project_mut(&mut self) -> &mut Project {
        Arc::make_mut(&mut self.project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_is_reported() {
        let mut session = Session::new();
        let error = session.execute("does.not.exist", &Value::Null).unwrap_err();
        assert!(matches!(error, EngineError::UnknownCommand(_)));
    }

    #[test]
    fn project_new_clears_history() {
        let mut session = Session::new();
        session.undo();
        session
            .execute("project.new", &serde_json::json!({"name": "Board"}))
            .unwrap();
        assert_eq!(session.project().name(), "Board");
        assert!(!session.can_undo());
        assert!(!session.can_redo());
        assert!(!session.dirty());
    }

    #[test]
    fn failed_undo_returns_false() {
        let mut session = Session::new();
        assert!(!session.undo());
        assert!(!session.redo());
    }
}
