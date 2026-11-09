//! axl-mujoco: a small safe layer over `mujoco-rs` for the AXL sim thread.
//!
//! This is the only crate in the workspace that may contain `unsafe`. The
//! `mujoco-rs` wrappers already cover everything AXL needs, so today it holds
//! none; any future `unsafe` block must carry a `// SAFETY:` comment.
//!
//! [`Physics`] owns one MuJoCo model and its data. It is not `Sync`: only the
//! sim thread may touch it (see `SPEC.md`, Architecture).
//!
//! Units are SI and the frame is right-handed Z-up, as in MuJoCo.

#![warn(missing_docs)]

pub mod alloc_count;
mod error;
mod physics;
mod types;

pub use error::PhysicsError;
pub use physics::Physics;
pub use types::{
    ActuatorId, BodyId, ContactInfo, GeomId, GroupMask, JointId, PhysicsState, PhysicsWarnings,
    Pose, RayHit, SensorId, SiteId,
};
