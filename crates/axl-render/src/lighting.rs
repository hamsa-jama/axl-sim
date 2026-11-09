//! Lighting configuration (`assets/render/lighting.ron`) and the procedural studio environment.
//!
//! Decision record 0005 explains why the environment cubemap is generated in code:
//! the sandbox that built this milestone cannot download the Poly Haven HDRI.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
    TextureViewDimension,
};
use serde::{Deserialize, Serialize};

use crate::materials_data::f32_to_f16_bits;

/// The shipped lighting file, used when no file is found on disk.
pub const DEFAULT_LIGHTING_RON: &str = include_str!("../../../assets/render/lighting.ron");

/// Errors from loading a render configuration file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("could not read {path}: {source}")]
    Io {
        /// File that failed.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The file is not valid RON for this type.
    #[error("could not parse {path}: {message}")]
    Parse {
        /// File that failed.
        path: String,
        /// What the parser said.
        message: String,
    },
    /// The file was made by a newer version of AXL.
    #[error("This file was made by a newer version of AXL.")]
    TooNew,
}

/// Read and parse a RON file.
pub fn load_ron<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.display().to_string(),
        source,
    })?;
    ron::de::from_str(&text).map_err(|e| ConfigError::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

/// The directory holding shipped assets (`assets/`).
///
/// Order: `AXL_ASSETS` environment variable, `assets/` beside the executable,
/// `assets/` in the current directory, then the repository's `assets/` (development).
pub fn assets_dir() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("AXL_ASSETS") {
        return p.into();
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("assets"));
    }
    candidates.push(std::path::PathBuf::from("assets"));
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
}

/// An sRGB colour as three floats in 0..=1.
pub type Srgb = (f32, f32, f32);

/// Convert to a Bevy colour.
pub fn srgb(c: Srgb) -> Color {
    Color::srgb(c.0, c.1, c.2)
}

/// The key light.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyLight {
    /// Elevation above the horizon in degrees (spec: 50).
    pub elevation_deg: f32,
    /// Direction the light comes from, measured around AXL +Z from +X.
    pub azimuth_deg: f32,
    /// Illuminance in lux.
    pub illuminance_lux: f32,
    /// Light colour (sRGB).
    pub color_srgb: Srgb,
    /// Last distance (m) that receives shadows.
    pub shadow_max_distance_m: f32,
    /// Far edge of the first cascade (m).
    pub shadow_first_cascade_m: f32,
}

/// Image-based lighting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Scale from the cubemap's relative radiance to cd/m2.
    pub intensity: f32,
    /// Backdrop colour (sRGB).
    pub backdrop_srgb: Srgb,
}

/// Physical camera settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraConfig {
    /// F-number.
    pub aperture_f: f32,
    /// Shutter time in seconds.
    pub shutter_s: f32,
    /// ISO sensitivity.
    pub iso: f32,
    /// Vertical field of view in degrees (spec: 35).
    pub fov_deg: f32,
    /// Near plane in metres (spec: 5 mm).
    pub near_m: f32,
    /// Far plane in metres (spec: 100 m).
    pub far_m: f32,
    /// Colour saturation applied after AgX tonemapping (1.0 is neutral). AgX desaturates highlights.
    pub post_saturation: f32,
}

impl CameraConfig {
    /// EV at ISO 100 for these settings.
    pub fn ev100(&self) -> f32 {
        (self.aperture_f * self.aperture_f * 100.0 / (self.shutter_s * self.iso)).log2()
    }
}

/// The floor's surface and how strongly grid lines print on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloorConfig {
    /// Base colour (sRGB).
    pub base_srgb: Srgb,
    /// Perceptual roughness (spec: 0.75, matte vinyl).
    pub roughness: f32,
    /// Reflectance.
    pub reflectance: f32,
    /// Darkening of minor lines at full opacity, 0..=1.
    pub minor_ink: f32,
    /// Darkening of major lines.
    pub major_ink: f32,
    /// Darkening of the bold metre lines.
    pub bold_ink: f32,
}

/// Bloom settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BloomConfig {
    /// Bloom intensity.
    pub intensity: f32,
}

/// The whole `lighting.ron` file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Resource)]
pub struct LightingConfig {
    /// File format version.
    pub version: u32,
    /// Key light.
    pub key_light: KeyLight,
    /// Environment light.
    pub environment: EnvironmentConfig,
    /// Camera and exposure.
    pub camera: CameraConfig,
    /// Floor surface.
    pub floor: FloorConfig,
    /// Bloom.
    pub bloom: BloomConfig,
}

impl LightingConfig {
    /// Parse RON text, refusing files from a newer version.
    pub fn from_ron(text: &str) -> Result<Self, ConfigError> {
        let cfg: LightingConfig = ron::de::from_str(text).map_err(|e| ConfigError::Parse {
            path: "lighting.ron".into(),
            message: e.to_string(),
        })?;
        if cfg.version > 1 {
            return Err(ConfigError::TooNew);
        }
        Ok(cfg)
    }

    /// Load `assets/render/lighting.ron`, falling back to the built-in copy.
    pub fn load_or_default() -> Self {
        let path = assets_dir().join("render/lighting.ron");
        match std::fs::read_to_string(&path) {
            Ok(text) => match Self::from_ron(&text) {
                Ok(c) => return c,
                Err(e) => tracing::warn!("{e}; using the built-in lighting"),
            },
            Err(e) => tracing::debug!("no {}: {e}; using the built-in lighting", path.display()),
        }
        // The built-in copy is part of the source tree and covered by a test.
        Self::from_ron(DEFAULT_LIGHTING_RON).unwrap_or_else(|e| panic!("built-in lighting: {e}"))
    }

    /// Direction the key light travels (from the light towards the scene), in the AXL frame.
    pub fn key_light_direction_axl(&self) -> Vec3 {
        let el = self.key_light.elevation_deg.to_radians();
        let az = self.key_light.azimuth_deg.to_radians();
        // Vector towards the light, then flipped.
        -Vec3::new(el.cos() * az.cos(), el.cos() * az.sin(), el.sin())
    }
}

// ---------------------------------------------------------------------------------------
// Procedural studio environment
// ---------------------------------------------------------------------------------------

/// A rectangular softbox seen from the studio centre.
///
/// `az` and `el` (degrees, AXL azimuth from +X and elevation) give the panel's centre
/// direction; `half_w` and `half_h` are half-extents on the tangent plane (a panel of
/// half-width 0.5 spans about 53 degrees); `power` is the relative radiance.
struct Softbox {
    az: f32,
    el: f32,
    half_w: f32,
    half_h: f32,
    power: f32,
    /// 0 is a flat panel; 1 fades the lower edge to nothing, like a graduated softbox.
    grad: f32,
    tint: Vec3,
}

/// The studio's panels: a large overhead key, tall side fills at both sides and a rim strip
/// behind the set. Tall panels reach down to the floor so vertical faces reflect them too.
/// A vertical strip light (light-tent style), floor to about 50 degrees, centred on azimuth `az`.
/// Eight of them around the set give every vertical face a softbox to mirror whatever its
/// orientation, with dark gaps between the strips so reflections have crisp edges to roll along.
const fn strip(az: f32, power: f32) -> Softbox {
    Softbox {
        az,
        el: 22.0,
        half_w: 0.06,
        half_h: 0.50,
        power,
        grad: 0.5,
        tint: Vec3::new(1.0, 0.99, 0.97),
    }
}

const SOFTBOXES: [Softbox; 12] = [
    strip(22.0, 300.0),
    strip(67.0, 300.0),
    strip(157.0, 300.0),
    strip(-23.0, 300.0),
    strip(-68.0, 300.0),
    strip(-158.0, 300.0),
    strip(112.0, 300.0),
    strip(-113.0, 300.0),
    // Large overhead key, slightly towards the key light's side.
    Softbox {
        az: -50.0,
        el: 72.0,
        half_w: 0.35,
        half_h: 0.26,
        power: 260.0,
        grad: 0.7,
        tint: Vec3::new(1.0, 0.97, 0.92),
    },
    // Side fill, right of camera, floor to high.
    Softbox {
        az: 100.0,
        el: 12.0,
        half_w: 0.10,
        half_h: 0.60,
        power: 80.0,
        grad: 0.4,
        tint: Vec3::new(0.92, 0.96, 1.0),
    },
    // Side fill, left of camera.
    Softbox {
        az: -125.0,
        el: 12.0,
        half_w: 0.10,
        half_h: 0.60,
        power: 55.0,
        grad: 0.4,
        tint: Vec3::new(1.0, 0.99, 0.97),
    },
    // Rim / back strip, wide and low.
    Softbox {
        az: 180.0,
        el: 30.0,
        half_w: 0.90,
        half_h: 0.10,
        power: 260.0,
        grad: 0.8,
        tint: Vec3::new(1.0, 1.0, 1.0),
    },
];

/// Unit vector (Bevy world space, +Y up) for an AXL azimuth and elevation in degrees.
fn dir_from_az_el(az_deg: f32, el_deg: f32) -> Vec3 {
    let (az, el) = (az_deg.to_radians(), el_deg.to_radians());
    // AXL (x, y, z) maps to Bevy (x, z, -y).
    Vec3::new(el.cos() * az.cos(), el.sin(), -el.cos() * az.sin())
}

/// Coverage (0..=1) of one softbox in direction `d`, with an edge about 1.2% of the panel size wide.
fn softbox_mask(b: &Softbox, d: Vec3) -> f32 {
    let c = dir_from_az_el(b.az, b.el);
    let along = d.dot(c);
    if along <= 0.05 {
        return 0.0;
    }
    // Panel axes: horizontal tangent and the vertical tangent that completes the frame.
    let u = Vec3::Y.cross(c).normalize_or_zero();
    let u = if u == Vec3::ZERO { Vec3::X } else { u };
    let v = c.cross(u).normalize_or_zero();
    let (x, y) = (d.dot(u) / along, d.dot(v) / along);
    let soft = 0.012;
    let edge =
        |p: f32, half: f32| 1.0 - smooth((p.abs() - half) / (half * soft * 6.0 + 1e-4) + 0.5);
    let t = (y / b.half_h * 0.5 + 0.5).clamp(0.0, 1.0);
    edge(x, b.half_w) * edge(y, b.half_h) * (1.0 - b.grad + b.grad * t)
}

/// Relative radiance of the studio seen along `dir` (Bevy world space, +Y up).
///
/// A dim neutral room with a white floor bounce, lit by the [`SOFTBOXES`]. Values are
/// relative: the camera's `GeneratedEnvironmentMapLight` intensity scales them to cd/m2.
pub fn studio_radiance(dir: Vec3) -> Vec3 {
    let d = dir.normalize_or_zero();
    let sky = d.y.clamp(-1.0, 1.0);
    // Walls and ceiling are dim; the floor is a bright bounce that fades to dark below.
    let base = if sky >= 0.0 {
        0.05 + 0.05 * sky
    } else {
        0.30 - 0.14 * (-sky)
    };
    let mut c = Vec3::splat(base);
    // A light-tent dome: radiance climbs smoothly from the horizon to 50 degrees elevation and
    // eases off overhead. Flat faces see a gradient across their width and rounded edges sweep
    // through it, which is what makes bevels read as moulded plastic.
    let el = sky.asin().to_degrees();
    let dome = smooth((el - 8.0) / 42.0) * (1.0 - 0.7 * smooth((el - 60.0) / 25.0));
    c += Vec3::new(1.0, 0.99, 0.97) * 70.0 * dome;
    for b in &SOFTBOXES {
        c += b.tint * b.power * softbox_mask(b, d);
    }
    c
}

fn smooth(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Direction through texel `(x, y)` of cube face `face` (order +X -X +Y -Y +Z -Z), as in the
/// graphics-API cubemap convention.
pub fn cube_texel_direction(face: usize, x: usize, y: usize, size: usize) -> Vec3 {
    let s = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    let t = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    match face {
        0 => Vec3::new(1.0, -t, -s),
        1 => Vec3::new(-1.0, -t, s),
        2 => Vec3::new(s, 1.0, t),
        3 => Vec3::new(s, -1.0, -t),
        4 => Vec3::new(s, -t, 1.0),
        _ => Vec3::new(-s, -t, -1.0),
    }
    .normalize()
}

/// Build the studio environment as an `Rgba16Float` cubemap of `size` x `size` x 6.
pub fn studio_cubemap(size: usize) -> Image {
    let mut data: Vec<u8> = Vec::with_capacity(size * size * 6 * 8);
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let c = studio_radiance(cube_texel_direction(face, x, y, size));
                for v in [c.x, c.y, c.z, 1.0] {
                    data.extend_from_slice(&f32_to_f16_bits(v).to_le_bytes());
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The runtime filter copies from this texture, so it needs COPY_SRC.
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..Default::default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_lighting_file_parses() {
        let cfg = LightingConfig::from_ron(DEFAULT_LIGHTING_RON).unwrap();
        assert_eq!(cfg.key_light.elevation_deg, 50.0);
        assert_eq!(cfg.camera.fov_deg, 35.0);
        assert_eq!(cfg.camera.near_m, 0.005);
        assert_eq!(cfg.camera.far_m, 100.0);
        assert_eq!(cfg.floor.roughness, 0.75);
    }

    #[test]
    fn newer_version_is_refused() {
        let text = DEFAULT_LIGHTING_RON.replace("version: 1,", "version: 2,");
        assert!(matches!(
            LightingConfig::from_ron(&text),
            Err(ConfigError::TooNew)
        ));
    }

    #[test]
    fn exposure_follows_physical_camera() {
        let c = CameraConfig {
            aperture_f: 4.0,
            shutter_s: 1.0 / 125.0,
            iso: 100.0,
            fov_deg: 35.0,
            near_m: 0.005,
            far_m: 100.0,
            post_saturation: 1.0,
        };
        assert!((c.ev100() - 2000.0f32.log2()).abs() < 1e-4);
    }

    #[test]
    fn key_light_elevation_is_fifty_degrees_and_points_down() {
        let cfg = LightingConfig::from_ron(DEFAULT_LIGHTING_RON).unwrap();
        let d = cfg.key_light_direction_axl();
        assert!((d.length() - 1.0).abs() < 1e-5);
        assert!(d.z < 0.0);
        let elevation = (-d.z).asin().to_degrees();
        assert!((elevation - 50.0).abs() < 1e-3);
    }

    #[test]
    fn cube_directions_hit_the_right_axes_at_face_centres() {
        let n = 64;
        let c = n / 2;
        let near = |a: Vec3, b: Vec3| (a - b).length() < 0.08;
        assert!(near(cube_texel_direction(0, c, c, n), Vec3::X));
        assert!(near(cube_texel_direction(1, c, c, n), Vec3::NEG_X));
        assert!(near(cube_texel_direction(2, c, c, n), Vec3::Y));
        assert!(near(cube_texel_direction(3, c, c, n), Vec3::NEG_Y));
        assert!(near(cube_texel_direction(4, c, c, n), Vec3::Z));
        assert!(near(cube_texel_direction(5, c, c, n), Vec3::NEG_Z));
    }

    #[test]
    fn adjacent_cube_faces_agree_at_their_seam() {
        // The +X face's right edge (s = +1) touches the -Z face's left edge, and so on.
        let n = 64;
        let a = cube_texel_direction(0, n - 1, n / 2, n);
        let b = cube_texel_direction(5, 0, n / 2, n);
        assert!((a - b).length() < 0.06, "{a} vs {b}");
        let a = cube_texel_direction(4, n - 1, n / 2, n);
        let b = cube_texel_direction(0, 0, n / 2, n);
        assert!((a - b).length() < 0.06, "{a} vs {b}");
    }

    #[test]
    fn studio_is_finite_and_has_bright_softboxes() {
        let mut peak = 0.0f32;
        for face in 0..6 {
            for y in (0..32).step_by(3) {
                for x in (0..32).step_by(3) {
                    let c = studio_radiance(cube_texel_direction(face, x, y, 32));
                    assert!(c.is_finite() && c.min_element() > 0.0);
                    peak = peak.max(c.x);
                }
            }
        }
        assert!(
            peak > 5.0,
            "softboxes should be much brighter than the room: {peak}"
        );
    }

    #[test]
    fn cubemap_has_expected_size() {
        let img = studio_cubemap(16);
        assert_eq!(img.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(img.data.as_ref().unwrap().len(), 16 * 16 * 6 * 8);
    }
}
