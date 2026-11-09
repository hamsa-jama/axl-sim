//! Errors from the physics layer.

use thiserror::Error;

/// Everything that can go wrong in [`crate::Physics`].
#[derive(Debug, Error)]
pub enum PhysicsError {
    /// MuJoCo could not compile the MJCF text.
    #[error("MuJoCo could not load the model: {0}")]
    Load(String),
    /// A saved state does not belong to this model.
    #[error("saved state has {got} values but this model needs {expected}")]
    StateSize {
        /// Values in the saved state.
        got: usize,
        /// Values the model needs.
        expected: usize,
    },
    /// MuJoCo refused to write a state back.
    #[error("could not restore the saved state: {0}")]
    Restore(String),
    /// A name did not match anything in the model.
    #[error("the model has no {kind} called \"{name}\"")]
    UnknownName {
        /// What was looked up (`body`, `joint`, ...).
        kind: &'static str,
        /// The name that was not found.
        name: String,
    },
}
