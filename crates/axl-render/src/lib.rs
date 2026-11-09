//! axl-render: Bevy rendering for AXL.
//!
//! This crate owns the look of the simulator: lighting and the studio environment,
//! materials, the grid floor, the view camera, quality presets and the benchmark that
//! picks one, the body sync from simulation snapshots, and the one conversion between
//! AXL's Z-up frame and Bevy's Y-up frame (see [`frames`]).
//!
//! Only `axl-app` and `axl-render` may depend on Bevy.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod browser;
pub mod camera_input;
pub mod camera_rig;
pub mod frames;
pub mod grid;
pub mod grid_math;
pub mod lighting;
pub mod materials_data;
pub mod parts;
pub mod placeholder;
pub mod quality;
pub mod scene;
pub mod stage;
pub mod sync;

pub use bevy;
pub use stage::{BenchmarkFinished, RenderOptions, StageContent, StagePlugin};
