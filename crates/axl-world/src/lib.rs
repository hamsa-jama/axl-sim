//! axl-world: worlds, standard objects and arena building (SPEC.md, "Worlds and physical objects").
//!
//! A world is a RON file in `assets/worlds/`: floor mat, walls, static shapes, loose objects, the
//! spawn pose and named zones for grading. [`build_world`] turns one into the MJCF the compiler
//! places inside `<worldbody>`, plus the zones and the rasterised mat the colour sensor reads. No
//! Bevy, no MuJoCo.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod build;
pub mod def;
pub mod library;
pub mod maze;
pub mod objects;

pub use build::{
    BuiltWorld, MatRaster, ObjectVisual, ShapeKind, ShapeVisual, WorldFriction, build_world,
    display_mat, euler_xyz_quat,
};
pub use def::{
    MatDef, MatShape, MazeDef, ObjectPlacement, Pose2, Primitive, StaticShape, VariantDef, WallDef,
    WorldDef, Zone, ZoneShape,
};
pub use library::{WorldError, builtin_objects, builtin_worlds, world};
pub use objects::{ObjShape, ObjectDef};
