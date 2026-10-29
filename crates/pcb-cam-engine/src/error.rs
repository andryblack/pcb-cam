//! Errors returned by the command engine.

/// A failure while executing a command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    /// The command id is not registered.
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    /// The command exists but is not available in the current state.
    #[error("command `{id}` is unavailable: {reason}")]
    Disabled {
        /// Command id.
        id: String,
        /// Why the command cannot run.
        reason: String,
    },
    /// The command parameters do not fit the schema.
    #[error("invalid parameters: {0}")]
    BadParams(String),
    /// The command failed.
    #[error("{0}")]
    Command(String),
}
