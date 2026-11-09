//! Placeholder shapes for the look-dev scene and tests.
//!
//! Real part meshes (bevelled bricks, involute gears, treaded tyres) arrive in
//! milestone M3 as plain vertex data from `axl-parts`. Until then this module builds
//! simple shapes in the AXL frame with the same look-relevant properties: chamfered
//! edges that catch light, and UVs that tile the micro-surface texture.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Size in metres that one tile of the micro-surface texture covers.
pub const MICRO_TILE_M: f32 = 0.016;

/// A box with chamfered edges and corners, centred on the origin.
///
/// `half` are the half extents and `bevel` is the chamfer width, all in metres.
/// Chamfers use flat normals, which gives a crisp highlight line along every edge.
pub fn bevel_box_mesh(half: Vec3, bevel: f32) -> Mesh {
    let b = bevel.clamp(0.0, 0.45 * half.min_element());
    let inner = half - Vec3::splat(b);
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();

    // Push a polygon (fan from vertex 0), flipping the winding so it faces `normal`.
    let mut push = |verts: &[Vec3], normal: Vec3| {
        let geo = (verts[1] - verts[0]).cross(verts[2] - verts[0]);
        let order: Vec<usize> = if geo.dot(normal) >= 0.0 {
            (0..verts.len()).collect()
        } else {
            (0..verts.len()).rev().collect()
        };
        let n = normal.normalize();
        // Planar UV from the dominant axis of the normal, in micro-texture tiles.
        let abs = n.abs();
        let project = |p: Vec3| -> [f32; 2] {
            let (u, v) = if abs.x >= abs.y && abs.x >= abs.z {
                (p.y, p.z)
            } else if abs.y >= abs.z {
                (p.x, p.z)
            } else {
                (p.x, p.y)
            };
            [u / MICRO_TILE_M, v / MICRO_TILE_M]
        };
        for k in 1..order.len() - 1 {
            for &i in &[order[0], order[k], order[k + 1]] {
                let p = verts[i];
                pos.push(p.to_array());
                nor.push(n.to_array());
                uv.push(project(p));
            }
        }
    };

    let axis = [Vec3::X, Vec3::Y, Vec3::Z];
    // Six main faces, inset by the bevel.
    for a in 0..3 {
        for s in [-1.0f32, 1.0] {
            let (b1, b2) = ((a + 1) % 3, (a + 2) % 3);
            let corner = |sa: f32, sb: f32| -> Vec3 {
                axis[a] * s * half[a] + axis[b1] * sa * inner[b1] + axis[b2] * sb * inner[b2]
            };
            push(
                &[
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                    corner(1.0, 1.0),
                    corner(-1.0, 1.0),
                ],
                axis[a] * s,
            );
        }
    }
    if b > 0.0 {
        // Twelve edge chamfers.
        for a in 0..3 {
            for bb in (a + 1)..3 {
                let c = 3 - a - bb;
                for sa in [-1.0f32, 1.0] {
                    for sb in [-1.0f32, 1.0] {
                        let p1 = |sc: f32| {
                            axis[a] * sa * half[a]
                                + axis[bb] * sb * inner[bb]
                                + axis[c] * sc * inner[c]
                        };
                        let p2 = |sc: f32| {
                            axis[a] * sa * inner[a]
                                + axis[bb] * sb * half[bb]
                                + axis[c] * sc * inner[c]
                        };
                        let n = axis[a] * sa + axis[bb] * sb;
                        push(&[p1(-1.0), p1(1.0), p2(1.0), p2(-1.0)], n);
                    }
                }
            }
        }
        // Eight corner triangles.
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let vx = Vec3::new(sx * half.x, sy * inner.y, sz * inner.z);
                    let vy = Vec3::new(sx * inner.x, sy * half.y, sz * inner.z);
                    let vz = Vec3::new(sx * inner.x, sy * inner.y, sz * half.z);
                    push(&[vx, vy, vz], Vec3::new(sx, sy, sz));
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    if mesh.generate_tangents().is_err() {
        tracing::warn!("could not generate tangents for a placeholder box");
    }
    mesh
}

/// A cylinder along AXL Z, centred on the origin, with UVs in micro tiles.
pub fn cylinder_mesh(radius: f32, length: f32) -> Mesh {
    // Bevy's cylinder runs along Y; turn it to Z.
    let mut mesh = Mesh::from(Cylinder::new(radius, length).mesh().resolution(64))
        .rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    scale_uvs(&mut mesh, radius.max(length * 0.5) * 2.0 / MICRO_TILE_M);
    if mesh.generate_tangents().is_err() {
        tracing::warn!("could not generate tangents for a placeholder cylinder");
    }
    mesh
}

/// A UV sphere with UVs in micro tiles.
pub fn sphere_mesh(radius: f32) -> Mesh {
    let mut mesh = Sphere::new(radius)
        .mesh()
        .uv(64, 32)
        .rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    scale_uvs(&mut mesh, radius * 2.0 / MICRO_TILE_M);
    if mesh.generate_tangents().is_err() {
        tracing::warn!("could not generate tangents for a placeholder sphere");
    }
    mesh
}

/// A placeholder spur gear: a module-1 mm blank (pitch diameter in mm equals the tooth count)
/// with plain rectangular teeth. The real involute teeth come with M3.
pub fn gear_blank_mesh(teeth: u32, thickness: f32) -> Mesh {
    let pitch_r = teeth as f32 * 0.001 * 0.5; // module 1 mm
    let root_r = pitch_r - 0.00125;
    let mut mesh = cylinder_mesh(root_r, thickness);
    let tooth_w = std::f32::consts::PI * 0.001 * 0.5; // half the circular pitch
    let tooth_h = 0.00225;
    for i in 0..teeth {
        let a = i as f32 / teeth as f32 * std::f32::consts::TAU;
        let r = root_r + tooth_h * 0.5 - 0.0002;
        let tooth = bevel_box_mesh(
            Vec3::new(tooth_h * 0.5, tooth_w * 0.5, thickness * 0.5),
            0.00015,
        )
        .transformed_by(
            Transform::from_translation(Vec3::new(r * a.cos(), r * a.sin(), 0.0))
                .with_rotation(Quat::from_rotation_z(a)),
        );
        mesh.merge(&tooth).ok();
    }
    mesh
}

/// Multiply every UV by `factor`.
pub fn scale_uvs(mesh: &mut Mesh, factor: f32) {
    if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    {
        for uv in uvs.iter_mut() {
            uv[0] *= factor;
            uv[1] *= factor;
        }
    }
}

/// Total bounding box of a mesh's positions (min, max).
pub fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for p in pos {
        lo = lo.min(Vec3::from(*p));
        hi = hi.max(Vec3::from(*p));
    }
    Some((lo, hi))
}

/// Triangle indices are not used by these meshes; this keeps the import used in tests.
#[allow(dead_code)]
fn _indices(_: Indices) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bevel_box_spans_the_requested_size() {
        let half = Vec3::new(0.0159, 0.0079, 0.0048);
        let mesh = bevel_box_mesh(half, 0.00015);
        let (lo, hi) = mesh_bounds(&mesh).unwrap();
        assert!((hi - half).abs().max_element() < 1e-7);
        assert!((lo + half).abs().max_element() < 1e-7);
    }

    #[test]
    fn bevel_box_has_26_faces_worth_of_triangles() {
        // 6 quads (2 tris) + 12 quads (2 tris) + 8 triangles = 44 triangles.
        let mesh = bevel_box_mesh(Vec3::splat(0.01), 0.001);
        let n = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().len();
        assert_eq!(n, 44 * 3);
        // Zero bevel is a plain box: 12 triangles.
        let plain = bevel_box_mesh(Vec3::splat(0.01), 0.0);
        assert_eq!(
            plain.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().len(),
            12 * 3
        );
    }

    #[test]
    fn normals_face_outwards_and_are_unit_length() {
        let mesh = bevel_box_mesh(Vec3::new(0.02, 0.01, 0.005), 0.001);
        let pos = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let nor = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x3(v) => v.clone(),
            _ => panic!(),
        };
        for tri in 0..pos.len() / 3 {
            let (a, b, c) = (
                Vec3::from(pos[tri * 3]),
                Vec3::from(pos[tri * 3 + 1]),
                Vec3::from(pos[tri * 3 + 2]),
            );
            let n = Vec3::from(nor[tri * 3]);
            assert!((n.length() - 1.0).abs() < 1e-5);
            // Winding agrees with the stored normal, and the face is on the outside of the box.
            assert!((b - a).cross(c - a).dot(n) > 0.0);
            assert!(((a + b + c) / 3.0).dot(n) > 0.0);
        }
    }

    #[test]
    fn gear_pitch_diameter_in_mm_is_the_tooth_count() {
        let mesh = gear_blank_mesh(24, 0.0032);
        let (lo, hi) = mesh_bounds(&mesh).unwrap();
        let outer_d = (hi.x - lo.x).max(hi.y - lo.y);
        // Outside diameter = pitch diameter + 2 module = 26 mm for 24 teeth (within tooth tip shape).
        assert!((outer_d - 0.026).abs() < 0.0006, "{outer_d}");
        assert!((hi.z - lo.z - 0.0032).abs() < 1e-6);
    }

    #[test]
    fn cylinder_runs_along_z() {
        let mesh = cylinder_mesh(0.028, 0.02);
        let (lo, hi) = mesh_bounds(&mesh).unwrap();
        assert!((hi.z - 0.01).abs() < 1e-5 && (lo.z + 0.01).abs() < 1e-5);
        assert!((hi.x - 0.028).abs() < 1e-4);
    }
}
