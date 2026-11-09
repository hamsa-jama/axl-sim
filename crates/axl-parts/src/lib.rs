//! axl-parts: part catalog types, RON loading, connectors and procedural mesh generation.
//!
//! Everything here is plain data and arithmetic: no Bevy, no GPU. Meshes are generated in
//! millimetres by small generators and handed to the renderer as [`trimesh::MeshData`]
//! (metres, f32).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod block;
pub mod build;
pub mod catalog;
pub mod devices;
pub mod earcut;
pub mod extrude;
pub mod generate;
pub mod lathe;
pub mod profile;
pub mod shapes;
pub mod trimesh;
pub mod types;

pub use catalog::{Catalog, PartsError};
pub use types::{PartDef, PartMesh};

impl PartDef {
    /// Generate this part's renderer meshes (metres, f32, part frame).
    pub fn build_mesh(&self) -> PartMesh {
        build::build_part_mesh(&self.shape)
    }

    /// Analytic solid volume in m3, for parts whose mass is computed from volume.
    pub fn solid_volume_m3(&self) -> Option<f64> {
        build::solid_volume_mm3(&self.shape).map(|v| v * 1e-9)
    }
}
