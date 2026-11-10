//! A mission's world as the live view draws it: the printed floor mat, walls and static shapes,
//! loose objects and the grading zones. Plain data (`WorldScene`) comes from the app; this module
//! turns it into entities under the sim stage root.
//!
//! Everything is in the AXL frame (Z up), like the rest of the scene under the root entity, so no
//! frame conversion happens here. Objects are [`SimBody`] entities, moved by the snapshots.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::sync::SimBody;

/// Height of the printed mat above the floor, metres (it is a sheet of vinyl, not a solid).
pub const MAT_Z_M: f32 = 0.0003;
/// Height of the zone markings above the floor, metres.
pub const ZONE_Z_M: f32 = 0.0007;

/// The printed mat as pixels: linear RGB, row 0 at the lowest y.
#[derive(Clone, Debug, PartialEq)]
pub struct MatImage {
    /// Lower corner, metres.
    pub min: [f64; 2],
    /// Upper corner, metres.
    pub max: [f64; 2],
    /// Pixels along x.
    pub width: usize,
    /// Pixels along y.
    pub height: usize,
    /// Row-major linear RGB.
    pub pixels: Vec<[f32; 3]>,
}

/// A fixed shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StaticKind {
    /// Box with these half sizes, metres.
    Box([f64; 3]),
    /// Cylinder along z: radius and half height, metres.
    Cylinder(f64, f64),
}

/// What a fixed shape is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticMaterial {
    /// Painted wood (walls).
    Wood,
    /// Plastic (some ramps).
    Plastic,
    /// Rubber (some ramps).
    Rubber,
}

impl StaticMaterial {
    /// From the world file's material name.
    pub fn parse(name: &str) -> Self {
        match name {
            "rubber" => Self::Rubber,
            "plastic" => Self::Plastic,
            _ => Self::Wood,
        }
    }
}

/// A wall, ramp or post.
#[derive(Clone, Debug, PartialEq)]
pub struct StaticDraw {
    /// Shape and size.
    pub kind: StaticKind,
    /// Centre, metres.
    pub pos: [f64; 3],
    /// Orientation `[w, x, y, z]`.
    pub quat: [f64; 4],
    /// sRGB colour.
    pub color: [u8; 3],
    /// Material.
    pub material: StaticMaterial,
}

/// The shape of a loose object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectKind {
    /// Cube with this side, metres.
    Cube(f64),
    /// Cylinder (axis up): diameter, height, metres.
    Cylinder(f64, f64),
    /// Sphere with this diameter, metres.
    Sphere(f64),
}

/// A loose object that moves with body `body` of the snapshots.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectDraw {
    /// MuJoCo body id.
    pub body: usize,
    /// Shape.
    pub kind: ObjectKind,
    /// sRGB colour.
    pub color: [u8; 3],
}

/// The area of a zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoneShape {
    /// Axis-aligned rectangle.
    Rect {
        /// Lower corner.
        min: [f64; 2],
        /// Upper corner.
        max: [f64; 2],
    },
    /// Circle.
    Circle {
        /// Centre.
        center: [f64; 2],
        /// Radius.
        radius: f64,
    },
}

/// A grading zone, drawn as a translucent patch with an outline.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneDraw {
    /// Name, for labels.
    pub name: String,
    /// Area.
    pub shape: ZoneShape,
}

/// The world of the running mission.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct WorldScene {
    /// The printed mat.
    pub mat: Option<MatImage>,
    /// Walls and static shapes.
    pub statics: Vec<StaticDraw>,
    /// Loose objects.
    pub objects: Vec<ObjectDraw>,
    /// Zones to mark on the floor.
    pub zones: Vec<ZoneDraw>,
}

impl WorldScene {
    /// Centre and radius (metres) of a circle around everything in the world, for framing the
    /// camera. `None` for an empty world.
    pub fn extent(&self) -> Option<([f64; 2], f64)> {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        let mut add = |x: f64, y: f64| {
            lo = [lo[0].min(x), lo[1].min(y)];
            hi = [hi[0].max(x), hi[1].max(y)];
        };
        if let Some(m) = &self.mat {
            add(m.min[0], m.min[1]);
            add(m.max[0], m.max[1]);
        }
        for s in &self.statics {
            let r = match s.kind {
                StaticKind::Box(h) => h[0].hypot(h[1]),
                StaticKind::Cylinder(r, _) => r,
            };
            add(s.pos[0] - r, s.pos[1] - r);
            add(s.pos[0] + r, s.pos[1] + r);
        }
        if lo[0] > hi[0] {
            return None;
        }
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        Some((c, (hi[0] - lo[0]).hypot(hi[1] - lo[1]) / 2.0))
    }
}

/// Marks everything that belongs to the world (despawned with the sim stage).
#[derive(Component)]
pub struct WorldPiece;

fn srgb_u8(v: f32) -> u8 {
    let c = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The mat as an 8-bit sRGB image, linear-filtered, clamped at the edges.
pub fn mat_image(m: &MatImage) -> Image {
    let mut data = Vec::with_capacity(m.pixels.len() * 4);
    for p in &m.pixels {
        data.extend([srgb_u8(p[0]), srgb_u8(p[1]), srgb_u8(p[2]), 255]);
    }
    let mut image = Image::new(
        Extent3d {
            width: m.width.max(1) as u32,
            height: m.height.max(1) as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A horizontal quad facing up, with UVs running from the lower corner (0, 0) to the upper one.
pub fn quad_mesh(min: [f32; 2], max: [f32; 2], z: f32) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [min[0], min[1], z],
            [max[0], min[1], z],
            [max[0], max[1], z],
            [min[0], max[1], z],
        ],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

fn quat_wxyz(q: [f64; 4]) -> Quat {
    let r = Quat::from_xyzw(q[1] as f32, q[2] as f32, q[3] as f32, q[0] as f32);
    if r.length_squared() > 0.0 {
        r.normalize()
    } else {
        Quat::IDENTITY
    }
}

fn color(c: [u8; 3]) -> Color {
    Color::srgb_u8(c[0], c[1], c[2])
}

/// Colour of the zone patches (a clear green, always with an outline, never colour alone).
const ZONE_RGB: [u8; 3] = [0x2E, 0xCC, 0x71];

/// Spawn the world under `parent`. Returns the entities of the loose objects.
pub fn spawn_world(
    commands: &mut Commands,
    parent: Entity,
    scene: &WorldScene,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    if let Some(m) = &scene.mat {
        let tex = images.add(mat_image(m));
        let mesh = meshes.add(quad_mesh(
            [m.min[0] as f32, m.min[1] as f32],
            [m.max[0] as f32, m.max[1] as f32],
            MAT_Z_M,
        ));
        // Matte vinyl with the mission graphics as its colour (SPEC, "Materials").
        let mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(tex),
            perceptual_roughness: 0.75,
            reflectance: 0.25,
            ..default()
        });
        let e = commands
            .spawn((
                WorldPiece,
                Name::new("mat"),
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::default(),
                NotShadowCaster,
            ))
            .id();
        commands.entity(parent).add_child(e);
    }

    for (i, s) in scene.statics.iter().enumerate() {
        let (mesh, extra) = match s.kind {
            StaticKind::Box(h) => (
                meshes.add(Cuboid::new(
                    (h[0] * 2.0) as f32,
                    (h[1] * 2.0) as f32,
                    (h[2] * 2.0) as f32,
                )),
                Quat::IDENTITY,
            ),
            StaticKind::Cylinder(r, hh) => (
                meshes.add(Cylinder::new(r as f32, (hh * 2.0) as f32)),
                // Bevy's cylinder stands along y; the shape stands along z.
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ),
        };
        let (rough, refl, clear) = match s.material {
            StaticMaterial::Wood => (0.6, 0.3, 0.0),
            StaticMaterial::Plastic => (0.3, 0.5, 0.15),
            StaticMaterial::Rubber => (0.85, 0.35, 0.0),
        };
        let base = match s.material {
            StaticMaterial::Rubber => Color::srgb_u8(0x1F, 0x1F, 0x1F),
            _ => color(s.color),
        };
        let mat = materials.add(StandardMaterial {
            base_color: base,
            perceptual_roughness: rough,
            reflectance: refl,
            clearcoat: clear,
            ..default()
        });
        let e = commands
            .spawn((
                WorldPiece,
                Name::new(format!("static_{i}")),
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(Vec3::new(
                    s.pos[0] as f32,
                    s.pos[1] as f32,
                    s.pos[2] as f32,
                ))
                .with_rotation(quat_wxyz(s.quat) * extra),
            ))
            .id();
        commands.entity(parent).add_child(e);
    }

    for (i, o) in scene.objects.iter().enumerate() {
        let (mesh, extra) = match o.kind {
            ObjectKind::Cube(s) => (meshes.add(Cuboid::from_length(s as f32)), Quat::IDENTITY),
            ObjectKind::Cylinder(d, h) => (
                meshes.add(Cylinder::new((d / 2.0) as f32, h as f32)),
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ),
            ObjectKind::Sphere(d) => (meshes.add(Sphere::new((d / 2.0) as f32)), Quat::IDENTITY),
        };
        let mat = materials.add(StandardMaterial {
            base_color: color(o.color),
            perceptual_roughness: 0.5,
            ..default()
        });
        let body = commands
            .spawn((
                WorldPiece,
                SimBody(o.body),
                Name::new(format!("object_{i}")),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        let shape = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_rotation(extra),
            ))
            .id();
        commands.entity(body).add_child(shape);
        commands.entity(parent).add_child(body);
    }

    let fill = materials.add(StandardMaterial {
        base_color: color(ZONE_RGB).with_alpha(0.28),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let line = materials.add(StandardMaterial {
        base_color: color(ZONE_RGB),
        unlit: true,
        ..default()
    });
    const EDGE: f32 = 0.004; // outline width, metres
    for z in &scene.zones {
        let piece = |commands: &mut Commands,
                     mesh: Handle<Mesh>,
                     m: &Handle<StandardMaterial>,
                     tf: Transform| {
            let e = commands
                .spawn((
                    WorldPiece,
                    Name::new(format!("zone_{}", z.name)),
                    Mesh3d(mesh),
                    MeshMaterial3d(m.clone()),
                    tf,
                    NotShadowCaster,
                ))
                .id();
            commands.entity(parent).add_child(e);
        };
        match z.shape {
            ZoneShape::Rect { min, max } => {
                let (lo, hi) = (
                    Vec2::new(min[0] as f32, min[1] as f32),
                    Vec2::new(max[0] as f32, max[1] as f32),
                );
                piece(
                    commands,
                    meshes.add(quad_mesh(lo.to_array(), hi.to_array(), ZONE_Z_M)),
                    &fill,
                    Transform::default(),
                );
                // Four outline strips.
                for (a, b) in [
                    (lo, Vec2::new(hi.x, lo.y + EDGE)),
                    (Vec2::new(lo.x, hi.y - EDGE), hi),
                    (lo, Vec2::new(lo.x + EDGE, hi.y)),
                    (Vec2::new(hi.x - EDGE, lo.y), hi),
                ] {
                    piece(
                        commands,
                        meshes.add(quad_mesh(a.to_array(), b.to_array(), ZONE_Z_M + 0.0002)),
                        &line,
                        Transform::default(),
                    );
                }
            }
            ZoneShape::Circle { center, radius } => {
                let at = Transform::from_translation(Vec3::new(
                    center[0] as f32,
                    center[1] as f32,
                    ZONE_Z_M,
                ));
                piece(
                    commands,
                    meshes.add(Circle::new(radius as f32).mesh().resolution(64)),
                    &fill,
                    at,
                );
                piece(
                    commands,
                    meshes.add(
                        Annulus::new(radius as f32 - EDGE, radius as f32)
                            .mesh()
                            .resolution(64),
                    ),
                    &line,
                    at.with_translation(at.translation + Vec3::Z * 0.0002),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extent_covers_the_mat_and_the_walls() {
        let mut w = WorldScene::default();
        assert_eq!(w.extent(), None);
        w.mat = Some(MatImage {
            min: [-1.0, -0.6],
            max: [1.0, 0.6],
            width: 1,
            height: 1,
            pixels: vec![[1.0; 3]],
        });
        let (c, r) = w.extent().unwrap();
        assert_eq!(c, [0.0, 0.0]);
        assert!((r - 1.0_f64.hypot(0.6)).abs() < 1e-9);
        w.statics.push(StaticDraw {
            kind: StaticKind::Box([0.05, 0.05, 0.05]),
            pos: [3.0, 0.0, 0.0],
            quat: [1.0, 0.0, 0.0, 0.0],
            color: [0; 3],
            material: StaticMaterial::Wood,
        });
        assert!(w.extent().unwrap().1 > 1.9);
    }

    #[test]
    fn linear_to_srgb_round_trips_the_ends_and_the_middle() {
        assert_eq!(srgb_u8(0.0), 0);
        assert_eq!(srgb_u8(1.0), 255);
        // Linear 0.2140 is sRGB 0.5.
        assert!((i32::from(srgb_u8(0.2140)) - 128).abs() <= 1);
    }

    #[test]
    fn the_mat_quad_runs_from_the_lower_to_the_upper_corner() {
        let m = quad_mesh([-1.0, -0.5], [1.0, 0.5], MAT_Z_M);
        let pos = m
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .unwrap();
        assert_eq!(pos[0], [-1.0, -0.5, MAT_Z_M]);
        assert_eq!(pos[2], [1.0, 0.5, MAT_Z_M]);
    }

    #[test]
    fn material_names_map_and_unknown_ones_fall_back_to_wood() {
        assert_eq!(StaticMaterial::parse("rubber"), StaticMaterial::Rubber);
        assert_eq!(StaticMaterial::parse("plastic"), StaticMaterial::Plastic);
        assert_eq!(StaticMaterial::parse("painted_wood"), StaticMaterial::Wood);
        assert_eq!(StaticMaterial::parse("???"), StaticMaterial::Wood);
    }
}
