//! Turning a [`ShapeGen`] into meshes and an analytic volume.

use crate::block::{BlockSpec, block_mesh};
use crate::devices::device_pieces;
use crate::shapes::*;
use crate::trimesh::TriMesh;
use crate::types::{MeshSlot, PartMesh, ShapeGen, Surface};

/// Plate height, mm.
pub const PLATE_MM: f64 = 3.2;

/// The block specification for a `ShapeGen::Block`.
pub fn block_spec(nx: u32, ny: u32, plates: u32, studs: bool, holes: bool) -> BlockSpec {
    let height = PLATE_MM * plates as f64;
    let n = nx.max(ny) as i32;
    let holes_y = if holes {
        // Holes along the long side (X), between studs, at mid height.
        (1..n)
            .map(|i| ((i as f64 - n as f64 / 2.0) * 8.0, height / 2.0))
            .collect()
    } else {
        vec![]
    };
    BlockSpec {
        nx,
        ny,
        height,
        studs,
        cavity: true,
        holes_y,
        bevel: PART_BEVEL,
    }
}

/// Generate every mesh of a shape as (body index, surface, mesh in mm).
pub fn shape_pieces(shape: &ShapeGen) -> Vec<(usize, Surface, TriMesh)> {
    match shape {
        ShapeGen::Block {
            nx,
            ny,
            plates,
            studs,
            holes,
        } => {
            vec![(
                0,
                Surface::Body,
                block_mesh(&block_spec(*nx, *ny, *plates, *studs, *holes)),
            )]
        }
        ShapeGen::Beam { shape } => vec![(0, Surface::Body, beam_mesh(*shape))],
        ShapeGen::Axle { units } => vec![(0, Surface::Body, axle_mesh(*units))],
        ShapeGen::Pin { units, kind } => vec![(0, Surface::Body, pin_mesh(*units, *kind))],
        ShapeGen::Gear { teeth, thickness } => {
            vec![(0, Surface::Body, gear_mesh(*teeth, thickness * 1000.0))]
        }
        ShapeGen::Wheel { diameter, width } => {
            let (rim, tyre) = wheel_meshes(diameter * 1000.0, width * 1000.0);
            vec![(0, Surface::Body, rim), (0, Surface::Rubber, tyre)]
        }
        ShapeGen::Caster { ball_diameter } => {
            let (housing, ball) = caster_meshes(ball_diameter * 1000.0);
            vec![(0, Surface::Body, housing), (0, Surface::Steel, ball)]
        }
        ShapeGen::Device { model } => device_pieces(*model),
    }
}

/// Build the renderer meshes of a shape (metres, f32). Pieces with the same body and surface
/// are merged into one slot.
pub fn build_part_mesh(shape: &ShapeGen) -> PartMesh {
    let mut merged: Vec<(usize, Surface, TriMesh)> = Vec::new();
    for (b, s, m) in shape_pieces(shape) {
        if let Some(e) = merged.iter_mut().find(|e| e.0 == b && e.1 == s) {
            e.2.append(&m);
        } else {
            merged.push((b, s, m));
        }
    }
    PartMesh {
        slots: merged
            .into_iter()
            .map(|(body, surface, m)| MeshSlot {
                body,
                surface,
                mesh: m.finish(),
            })
            .collect(),
    }
}

/// Analytic solid volume in mm3 for parts whose mass is computed from volume.
/// `None` for wheels, the caster and devices (their masses are measured or placeholders).
pub fn solid_volume_mm3(shape: &ShapeGen) -> Option<f64> {
    match shape {
        ShapeGen::Block {
            nx,
            ny,
            plates,
            studs,
            holes,
        } => Some(block_spec(*nx, *ny, *plates, *studs, *holes).volume()),
        ShapeGen::Beam { shape } => Some(beam_volume(*shape)),
        ShapeGen::Axle { units } => Some(axle_volume(*units)),
        ShapeGen::Pin { units, kind } => Some(pin_volume(*units, *kind)),
        ShapeGen::Gear { teeth, thickness } => Some(gear_volume(*teeth, thickness * 1000.0)),
        _ => None,
    }
}
