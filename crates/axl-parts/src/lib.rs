//! axl-parts: part catalog types, RON loading, connectors and procedural mesh generation.
//!
//! Everything here is plain data and arithmetic: no Bevy, no GPU. Meshes are generated in
//! millimetres by small generators and handed to the renderer as [`trimesh::MeshData`]
//! (metres, f32).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod block;
pub mod earcut;
pub mod extrude;
pub mod lathe;
pub mod profile;
pub mod shapes;
pub mod trimesh;
pub mod types;
