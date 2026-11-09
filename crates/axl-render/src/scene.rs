//! The look-dev scene (`assets/scenes/lookdev.ron`) and the 200-box stress scene.
//!
//! The look-dev scene is built from real parts; the stress scene uses chamfered boxes. Each part gets a `uid`, which seeds the small
//! per-part roughness and brightness variation, exactly as real parts will.

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use serde::{Deserialize, Serialize};

use crate::camera_rig::{ActiveQuality, CameraHome};
use crate::lighting::{ConfigError, assets_dir};
use crate::materials_data::{
    MICRO_NORMAL_SIZE, MICRO_NORMAL_STRENGTH, abs, micro_surface_normal_map_mips, mip_count,
    part_color, part_variation,
};
use crate::parts::PartLibrary;
use crate::placeholder::bevel_box_mesh;
use crate::sync::SimBody;

/// The shipped look-dev scene, used when no file is found on disk.
pub const DEFAULT_LOOKDEV_RON: &str = include_str!("../../../assets/scenes/lookdev.ron");

const MM: f32 = 0.001;

/// Surface type of a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MaterialKind {
    /// Opaque ABS plastic.
    Abs,
    /// Rubber tyre.
    Rubber,
    /// Steel (the caster ball).
    Steel,
}

fn zero3() -> (f32, f32, f32) {
    (0.0, 0.0, 0.0)
}

/// A real part placed in a scene. Lengths are millimetres, angles degrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartRef {
    /// Part id from `assets/parts`, for example `brick_2x4`.
    pub part: String,
    /// Position of the part frame origin, relative to the item, mm.
    #[serde(default = "zero3")]
    pub pos_mm: (f32, f32, f32),
    /// Rotation about X, Y, Z (applied as Rz * Ry * Rx).
    #[serde(default = "zero3")]
    pub rot_deg: (f32, f32, f32),
    /// Colour id such as `"red"`, or `"#RRGGBB"`.
    pub color: String,
}

/// A group of parts placed together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// Name for the entity.
    pub name: String,
    /// Position of the item origin, mm.
    pub pos_mm: (f32, f32, f32),
    /// Rotation about Z in degrees.
    #[serde(default)]
    pub yaw_deg: f32,
    /// The parts.
    pub parts: Vec<PartRef>,
}

/// Initial camera for a scene (AXL frame).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneCamera {
    /// Point looked at, mm.
    pub focus_mm: (f32, f32, f32),
    /// Direction around Z in degrees.
    pub yaw_deg: f32,
    /// Elevation in degrees.
    pub pitch_deg: f32,
    /// Distance in mm.
    pub distance_mm: f32,
}

/// The whole scene file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Resource)]
pub struct SceneDef {
    /// File format version.
    pub version: u32,
    /// Camera.
    pub camera: SceneCamera,
    /// Items.
    pub items: Vec<Item>,
}

impl SceneDef {
    /// Parse RON text, refusing newer versions.
    pub fn from_ron(text: &str) -> Result<Self, ConfigError> {
        let s: SceneDef = ron::de::from_str(text).map_err(|e| ConfigError::Parse {
            path: "lookdev.ron".into(),
            message: e.to_string(),
        })?;
        if s.version > 1 {
            return Err(ConfigError::TooNew);
        }
        Ok(s)
    }

    /// Load `assets/scenes/lookdev.ron`, or the built-in copy.
    pub fn load_lookdev() -> Self {
        let path = assets_dir().join("scenes/lookdev.ron");
        if let Ok(text) = std::fs::read_to_string(&path) {
            match Self::from_ron(&text) {
                Ok(s) => return s,
                Err(e) => tracing::warn!("{e}; using the built-in look-dev scene"),
            }
        }
        Self::from_ron(DEFAULT_LOOKDEV_RON)
            .unwrap_or_else(|e| panic!("built-in look-dev scene: {e}"))
    }

    /// The camera home this scene asks for.
    pub fn home(&self) -> CameraHome {
        let (x, y, z) = self.camera.focus_mm;
        CameraHome {
            focus: Vec3::new(x, y, z) * MM,
            yaw_deg: self.camera.yaw_deg,
            pitch_deg: self.camera.pitch_deg,
            distance: self.camera.distance_mm * MM,
        }
    }
}

/// Resolve a colour id (`"red"`) or hex string (`"#C91A09"`) to sRGB bytes.
pub fn resolve_color(id: &str) -> Option<[u8; 3]> {
    if let Some(hex) = id.strip_prefix('#') {
        if hex.len() == 6 {
            let v = u32::from_str_radix(hex, 16).ok()?;
            return Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
        }
        return None;
    }
    part_color(id)
}

/// Cache key: uid bucket, kind, colour, roughness and brightness steps.
type MaterialKey = (u32, MaterialKind, [u8; 3], i32, i32);

/// The micro-surface texture and every plastic material that uses it.
#[derive(Resource, Default)]
pub struct PartMaterials {
    micro: Handle<Image>,
    plastics: Vec<Handle<StandardMaterial>>,
    cache: BTreeMap<MaterialKey, Handle<StandardMaterial>>,
    meshes: BTreeMap<String, Handle<Mesh>>,
}

impl PartMaterials {
    /// Create the micro-surface image.
    pub fn new(images: &mut Assets<Image>) -> Self {
        let size = MICRO_NORMAL_SIZE;
        let data = micro_surface_normal_map_mips(size, MICRO_NORMAL_STRENGTH, 0x41584C);
        let level0 = size * size * 4;
        let mut image = Image::new(
            Extent3d {
                width: size as u32,
                height: size as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data[..level0].to_vec(),
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = mip_count(size);
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
        Self {
            micro: images.add(image),
            ..default()
        }
    }

    /// A shared material for (`kind`, `rgb`, per-part variation of `uid`).
    pub fn material(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        kind: MaterialKind,
        rgb: [u8; 3],
        uid: u32,
        micro_on: bool,
    ) -> Handle<StandardMaterial> {
        let var = part_variation(uid);
        // Quantise to three levels each (minus, none, plus) so that parts with similar variation
        // share a material: separate materials cannot batch, and 200 unique ones cost 200 draws in
        // every pass. Rounding to 0.03 roughness and 1.5% brightness steps keeps the spec's range.
        let rq = (var.roughness_delta / 0.03).round() as i32 * 3;
        let bq = ((var.brightness_scale - 1.0) / 0.015).round() as i32 * 15;
        let key = (0, kind, rgb, rq, bq);
        if let Some(h) = self.cache.get(&key) {
            return h.clone();
        }
        let rough_d = rq as f32 / 100.0;
        let bright = 1.0 + bq as f32 / 1000.0;
        let base = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).to_linear() * bright;
        let m = match kind {
            MaterialKind::Abs => StandardMaterial {
                base_color: base.into(),
                perceptual_roughness: abs::ROUGHNESS + rough_d,
                metallic: 0.0,
                reflectance: abs::REFLECTANCE,
                clearcoat: abs::CLEARCOAT,
                clearcoat_perceptual_roughness: abs::CLEARCOAT_ROUGHNESS,
                normal_map_texture: micro_on.then(|| self.micro.clone()),
                ..default()
            },
            MaterialKind::Rubber => StandardMaterial {
                base_color: Color::srgb_u8(0x1A, 0x1A, 0x1A),
                perceptual_roughness: 0.85 + rough_d,
                reflectance: 0.35,
                ..default()
            },
            MaterialKind::Steel => StandardMaterial {
                base_color: Color::srgb_u8(0xBF, 0xBF, 0xBF),
                metallic: 1.0,
                perceptual_roughness: 0.15 + rough_d,
                ..default()
            },
        };
        let h = materials.add(m);
        if kind == MaterialKind::Abs {
            self.plastics.push(h.clone());
        }
        self.cache.insert(key, h.clone());
        h
    }

    /// A shared mesh by key; `make` builds it the first time.
    pub fn mesh(
        &mut self,
        meshes: &mut Assets<Mesh>,
        key: String,
        make: impl FnOnce() -> Mesh,
    ) -> Handle<Mesh> {
        self.meshes
            .entry(key)
            .or_insert_with(|| meshes.add(make()))
            .clone()
    }
}

/// Switch the micro-surface normal map on or off on every plastic material when the preset changes.
pub fn apply_micro_surface(
    quality: Option<Res<ActiveQuality>>,
    disable: Res<crate::diagnostics::DisableSet>,
    parts: Option<Res<PartMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<Option<bool>>,
) {
    let (Some(quality), Some(parts)) = (quality, parts) else {
        return;
    };
    let on = quality.0.settings().micro_surface && !disable.micro_normal;
    if *last == Some(on) {
        return;
    }
    *last = Some(on);
    for h in &parts.plastics {
        if let Some(mut m) = materials.get_mut(h) {
            m.normal_map_texture = on.then(|| parts.micro.clone());
        }
    }
}

/// Everything needed to turn parts into entities.
pub struct ShapeBuilder<'a> {
    /// Mesh assets.
    pub meshes: &'a mut Assets<Mesh>,
    /// Material assets.
    pub materials: &'a mut Assets<StandardMaterial>,
    /// Shared material caches.
    pub parts: &'a mut PartMaterials,
    /// The part catalog and uploaded part meshes.
    pub lib: &'a mut PartLibrary,
    /// Whether micro-surface detail starts enabled.
    pub micro_on: bool,
}

/// Transform for a part placed with millimetres and Rz * Ry * Rx degrees.
pub fn part_transform(pos_mm: (f32, f32, f32), rot_deg: (f32, f32, f32)) -> Transform {
    Transform::from_translation(Vec3::new(pos_mm.0, pos_mm.1, pos_mm.2) * MM).with_rotation(
        Quat::from_euler(
            EulerRot::ZYX,
            rot_deg.2.to_radians(),
            rot_deg.1.to_radians(),
            rot_deg.0.to_radians(),
        ),
    )
}

impl ShapeBuilder<'_> {
    /// Spawn one item (a group of parts) under `root` and return its entity.
    pub fn spawn_item(
        &mut self,
        commands: &mut Commands,
        root: Entity,
        item: &Item,
        first_uid: u32,
    ) -> Entity {
        let (px, py, pz) = item.pos_mm;
        let parent = commands
            .spawn((
                Name::new(item.name.clone()),
                Transform::from_translation(Vec3::new(px, py, pz) * MM)
                    .with_rotation(Quat::from_rotation_z(item.yaw_deg.to_radians())),
                Visibility::default(),
            ))
            .id();
        commands.entity(root).add_child(parent);
        for (i, p) in item.parts.iter().enumerate() {
            let tf = part_transform(p.pos_mm, p.rot_deg);
            if self
                .spawn_part(
                    commands,
                    parent,
                    &p.part,
                    &p.color,
                    first_uid + i as u32,
                    tf,
                )
                .is_none()
            {
                tracing::warn!("unknown part {:?} in item {}", p.part, item.name);
            }
        }
        parent
    }

    fn materials_for(
        &mut self,
        kind: MaterialKind,
        rgb: [u8; 3],
        uid: u32,
    ) -> Handle<StandardMaterial> {
        self.parts
            .material(self.materials, kind, rgb, uid, self.micro_on)
    }
}

/// Spawn every item of `scene` under `root`. Returns the entity of the item named `rover`, if any.
pub fn spawn_scene(
    commands: &mut Commands,
    root: Entity,
    scene: &SceneDef,
    builder: &mut ShapeBuilder,
) -> Option<Entity> {
    let mut rover = None;
    let mut uid = 1;
    for item in &scene.items {
        let e = builder.spawn_item(commands, root, item, uid);
        uid += item.parts.len() as u32;
        if item.name == "rover" {
            rover = Some(e);
        }
    }
    rover
}

/// How many boxes the stress scene holds, set by `--stress-boxes`.
#[derive(Resource, Clone, Copy, Debug)]
pub struct StressBoxes(pub usize);

/// Spawn `n` small cubes in a loose grid, each driven by [`SimBody`] index `i`.
pub fn spawn_stress_boxes(
    commands: &mut Commands,
    root: Entity,
    n: usize,
    builder: &mut ShapeBuilder,
) {
    let colors = [
        "red",
        "blue",
        "yellow",
        "green",
        "orange",
        "white",
        "light_grey",
        "dark_grey",
        "black",
    ];
    let cols = (n as f32).sqrt().ceil() as usize;
    let mesh = builder.parts.mesh(builder.meshes, "stress box".into(), || {
        bevel_box_mesh(Vec3::splat(0.012), 0.0004)
    });
    for i in 0..n {
        let rgb = part_color(colors[i % colors.len()]).unwrap_or([200, 200, 200]);
        let mat = builder.materials_for(MaterialKind::Abs, rgb, i as u32 + 1);
        let (cx, cy) = ((i % cols) as f32, (i / cols) as f32);
        let half = cols as f32 * 0.5;
        let e = commands
            .spawn((
                Name::new(format!("box_{i}")),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz((cx - half) * 0.05, (cy - half) * 0.05, 0.012),
                SimBody(i),
            ))
            .id();
        commands.entity(root).add_child(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_lookdev_scene_parses_and_uses_real_parts() {
        let s = SceneDef::from_ron(DEFAULT_LOOKDEV_RON).unwrap();
        let names: Vec<&str> = s.items.iter().map(|i| i.name.as_str()).collect();
        for needed in ["brick_2x4", "beam_7", "gear_24", "wheel_56", "rover"] {
            assert!(names.contains(&needed), "missing {needed}");
        }
        let catalog = axl_parts::Catalog::builtin();
        for item in &s.items {
            for p in &item.parts {
                assert!(resolve_color(&p.color).is_some(), "bad colour {}", p.color);
                assert!(catalog.get(&p.part).is_some(), "unknown part {}", p.part);
            }
        }
    }

    #[test]
    fn the_rover_has_the_parts_the_spec_lists() {
        let s = SceneDef::from_ron(DEFAULT_LOOKDEV_RON).unwrap();
        let rover = s.items.iter().find(|i| i.name == "rover").unwrap();
        let count = |id: &str| rover.parts.iter().filter(|p| p.part == id).count();
        assert_eq!(count("hub"), 1);
        assert_eq!(count("motor_medium"), 2);
        assert_eq!(count("wheel_56"), 2);
        assert_eq!(count("caster_ball"), 1);
        assert_eq!(count("sensor_ultrasonic"), 1);
        assert_eq!(count("sensor_color"), 1);
    }

    #[test]
    fn the_scene_uses_the_requested_parts() {
        let s = SceneDef::from_ron(DEFAULT_LOOKDEV_RON).unwrap();
        let ids = |name: &str| -> Vec<String> {
            s.items
                .iter()
                .find(|i| i.name == name)
                .unwrap()
                .parts
                .iter()
                .map(|p| p.part.clone())
                .collect()
        };
        assert_eq!(ids("brick_2x4"), ["brick_2x4"]);
        assert_eq!(ids("beam_7"), ["beam_7"]);
        assert_eq!(ids("gear_24"), ["gear_24"]);
        assert_eq!(ids("wheel_56"), ["wheel_56"]);
    }

    #[test]
    fn part_rotation_order_is_z_y_x() {
        let tf = part_transform((0.0, 0.0, 0.0), (90.0, 0.0, 90.0));
        // Rx(90) turns +Z into -Y, then Rz(90) turns -Y into +X.
        assert!((tf.rotation * Vec3::Z - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn colour_resolution() {
        assert_eq!(resolve_color("red"), Some([0xC9, 0x1A, 0x09]));
        assert_eq!(resolve_color("#0055BF"), Some([0x00, 0x55, 0xBF]));
        assert_eq!(resolve_color("#12"), None);
        assert_eq!(resolve_color("nope"), None);
    }

    #[test]
    fn newer_scene_version_is_refused() {
        let text = DEFAULT_LOOKDEV_RON.replacen("version: 1", "version: 9", 1);
        assert!(matches!(
            SceneDef::from_ron(&text),
            Err(ConfigError::TooNew)
        ));
    }
}
