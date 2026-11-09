//! axl-build: the robot assembly. Placed parts on a 0.1 mm grid with 24 orientations, connections
//! derived from geometry, snapping, validation, undo and redo, and `.axlbot` files.
//!
//! Everything the Builder screen does is a method of [`builder::Builder`], so the UI and the tests
//! drive the same code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod builder;
pub mod geom;
pub mod io;
pub mod model;
pub mod orient;
pub mod place;
pub mod starter;
pub mod validate;

pub use builder::{BuildError, Builder, GhostKind};
pub use model::{Assembly, ConnRef, Connection, DeviceInstance, JointKind, PlacedPart, Port};
pub use orient::{Orient, TurnAxis};
