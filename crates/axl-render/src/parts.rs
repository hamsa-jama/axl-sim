//! Real part meshes: the part library (catalog plus uploaded meshes) and part spawning.
//!
//! `axl-parts` generates every mesh as plain vertex data; this module uploads each part type
//! once and shares it, so parts of the same type and colour batch automatically.

use std::collections::BTreeMap;

use axl_parts::trimesh::MeshData;
use axl_parts::types::{PartDef, Surface};
use axl_parts::Catalog;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::lighting::assets_dir;
use crate::scene::{MaterialKind, ShapeBuilder, resolve_color};

/// The uploaded meshes of one part type.
#[derive(Clone)]
pub struct CachedPart {
    /// One entry per slot: body index, surface, mesh.
    pub slots: Vec<(usize, Surface, Handle<Mesh>)>,
    /// Bounds (min, max) in the part frame, metres.
    pub bounds: ([f32; 3], [f32; 3]),
    /// Triangle count over all slots.
    pub triangles: usize,
}

/// The part catalog and its uploaded meshes.
#[derive(Resource)]
pub struct PartLibrary {
    catalog: Catalog,
    cache: BTreeMap<String, CachedPart>,
}

impl PartLibrary {
    /// Load `assets/parts/*.ron`, falling back to the built-in generator if the folder is missing.
    pub fn load() -> Self {
        let dir = assets_dir().join("parts");
        let catalog = match Catalog::load_dir(&dir) {
            Ok(c) if !c.is_empty() => c,
            Ok(_) => {
                tracing::warn!("{} has no parts; using the built-in catalog", dir.display());
                Catalog::builtin()
            }
            Err(e) => {
                tracing::warn!("{e}; using the built-in catalog");
                Catalog::builtin()
            }
        };
        Self {
            catalog,
            cache: BTreeMap::new(),
        }
    }

    /// The catalog.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The part's meshes, generated and uploaded on first use.
    pub fn cached(&mut self, id: &str, meshes: &mut Assets<Mesh>) -> Option<&CachedPart> {
        if !self.cache.contains_key(id) {
            let def = self.catalog.get(id)?;
            let pm = def.build_mesh();
            let bounds = pm.bounds();
            let triangles = pm.triangles();
            let slots = pm
                .slots
                .iter()
                .map(|s| (s.body, s.surface, meshes.add(to_bevy_mesh(&s.mesh))))
                .collect();
            self.cache.insert(id.to_string(), CachedPart { slots, bounds, triangles });
        }
        self.cache.get(id)
    }
}

/// Convert generated vertex data to a Bevy mesh (positions in the AXL frame, metres).
pub fn to_bevy_mesh(d: &MeshData) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, d.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, d.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, d.uvs.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, d.tangents.clone())
        .with_inserted_indices(Indices::U32(d.indices.clone()))
}

/// Rotation applied when a part is shown on its own: wheels stand up on their tyres.
pub fn display_rotation(def: &PartDef) -> Quat {
    match def.category {
        axl_parts::types::Category::Wheel => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        _ => Quat::IDENTITY,
    }
}

impl ShapeBuilder<'_> {
    /// Spawn one part under `parent` with transform `tf`. Returns the part's entity.
    pub fn spawn_part(
        &mut self,
        commands: &mut Commands,
        parent: Entity,
        part_id: &str,
        color: &str,
        uid: u32,
        tf: Transform,
    ) -> Option<Entity> {
        let default_color = self
            .lib
            .catalog()
            .get(part_id)
            .and_then(|d| d.colors.first().map(|c| c.0.clone()));
        let cached = self.lib.cached(part_id, self.meshes)?.clone();
        let rgb = resolve_color(color)
            .or_else(|| default_color.as_deref().and_then(resolve_color))
            .unwrap_or([200, 200, 200]);
        let entity = commands
            .spawn((Name::new(part_id.to_string()), tf, Visibility::default()))
            .id();
        commands.entity(parent).add_child(entity);
        for (_, surface, mesh) in &cached.slots {
            let (kind, rgb) = match surface {
                Surface::Body => (MaterialKind::Abs, rgb),
                Surface::Dark => (MaterialKind::Abs, [0x1B, 0x2A, 0x34]),
                Surface::Rubber => (MaterialKind::Rubber, [0x1A, 0x1A, 0x1A]),
                Surface::Steel => (MaterialKind::Steel, [0xBF, 0xBF, 0xBF]),
            };
            let mat = self
                .parts
                .material(self.materials, kind, rgb, uid, self.micro_on);
            let child = commands
                .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::default()))
                .id();
            commands.entity(entity).add_child(child);
        }
        Some(entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bevy_mesh_has_every_attribute() {
        let def = Catalog::builtin().get("brick_2x4").cloned().unwrap();
        let pm = def.build_mesh();
        let m = to_bevy_mesh(&pm.slots[0].mesh);
        assert!(m.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
        assert!(m.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert_eq!(
            m.indices().map(|i| i.len()),
            Some(pm.slots[0].mesh.indices.len())
        );
    }

    #[test]
    fn wheels_are_shown_standing_up() {
        let c = Catalog::builtin();
        let q = display_rotation(c.get("wheel_56").unwrap());
        assert!((q * Vec3::Z - Vec3::NEG_Y).length() < 1e-5);
        assert_eq!(display_rotation(c.get("brick_2x4").unwrap()), Quat::IDENTITY);
    }
}
