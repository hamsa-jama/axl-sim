//! The view camera: orbit / follow / top-down modes, view keys, exposure, and the
//! application of quality presets to the camera and key light.

use bevy::anti_alias::{
    fxaa::Fxaa,
    smaa::{Smaa, SmaaPreset},
    taa::TemporalAntiAliasing,
};
use bevy::camera::{Exposure, Hdr, OrthographicProjection, ScalingMode};
use bevy::camera_controller::pan_orbit_camera::prelude::*;
use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{
    CascadeShadowConfigBuilder, DirectionalLightShadowMap, GeneratedEnvironmentMapLight,
    ShadowFilteringMethod,
};
use bevy::pbr::{
    ContactShadows, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel,
};
use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::camera::{MipBias, TemporalJitter};
use bevy::render::view::{ColorGrading, ColorGradingGlobal};

use crate::frames::axl_to_bevy;
use crate::lighting::{LightingConfig, srgb};
use crate::quality::{AaMode, AoLevel, QualityPreset};

/// Marks the 3D view camera.
#[derive(Component)]
pub struct ViewCamera;

/// Marks the key light.
#[derive(Component)]
pub struct KeyLightMarker;

/// What the UI layer (egui) is currently using, so the 3D view ignores that input.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct UiCapture {
    /// The pointer is over a panel, or a widget is being dragged.
    pub pointer: bool,
    /// A text field or other widget has keyboard focus.
    pub keyboard: bool,
}

/// How the camera follows the world.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CameraMode {
    /// Free orbit around a focus point.
    #[default]
    Orbit,
    /// Orbit around, and move with, the followed entity.
    Follow,
    /// Straight down, orthographic, for mat missions.
    TopDown,
}

impl CameraMode {
    /// The next mode in the order orbit, follow, top-down (key `V`).
    pub fn next(self) -> Self {
        match self {
            Self::Orbit => Self::Follow,
            Self::Follow => Self::TopDown,
            Self::TopDown => Self::Orbit,
        }
    }

    /// Name for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Orbit => "Orbit",
            Self::Follow => "Follow robot",
            Self::TopDown => "Top-down",
        }
    }
}

/// Where `Home` returns the camera to, and what `F` focuses on. AXL frame.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraHome {
    /// Point looked at.
    pub focus: Vec3,
    /// Direction around Z in degrees (0 looks from +X towards the origin).
    pub yaw_deg: f32,
    /// Elevation above the horizon in degrees.
    pub pitch_deg: f32,
    /// Distance from the focus in metres.
    pub distance: f32,
}

impl Default for CameraHome {
    fn default() -> Self {
        Self {
            focus: Vec3::new(0.0, 0.0, 0.02),
            yaw_deg: -50.0,
            pitch_deg: 28.0,
            distance: 0.45,
        }
    }
}

/// Entity to centre on with `F` and to follow in follow mode.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct FocusTarget(pub Option<Entity>);

/// User camera preferences.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct CameraSettings {
    /// Reverse the direction of orbiting.
    pub invert_orbit: bool,
}

/// The active quality preset. Change it to re-apply every quality setting.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveQuality(pub QualityPreset);

/// Camera position for an orbit around `focus` (AXL frame, metres).
pub fn orbit_position(focus: Vec3, yaw_deg: f32, pitch_deg: f32, distance: f32) -> Vec3 {
    let (y, p) = (yaw_deg.to_radians(), pitch_deg.to_radians());
    focus + distance * Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), p.sin())
}

/// Place a camera transform on an orbit and look at the focus, with Bevy's +Y as up.
pub fn orbit_transform(home: &CameraHome) -> Transform {
    let pos = axl_to_bevy(orbit_position(
        home.focus,
        home.yaw_deg,
        home.pitch_deg,
        home.distance,
    ));
    Transform::from_translation(pos).looking_at(axl_to_bevy(home.focus), Vec3::Y)
}

/// Top-down placement: straight above the focus, AXL +Y pointing up the screen.
pub fn top_down_transform(focus: Vec3, height: f32) -> Transform {
    let f = axl_to_bevy(focus);
    // AXL +Y is Bevy -Z.
    Transform::from_translation(f + Vec3::Y * height).looking_at(f, Vec3::NEG_Z)
}

/// Bloom that only lets highlights glow: additive, with a threshold above the floor's brightness.
fn studio_bloom(intensity: f32) -> Bloom {
    Bloom {
        intensity,
        composite_mode: BloomCompositeMode::Additive,
        prefilter: BloomPrefilter {
            threshold: 1.3,
            threshold_softness: 0.2,
        },
        ..Bloom::NATURAL
    }
}

/// Spawn the view camera and the key light, applying `quality`.
pub fn spawn_view_camera(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    lighting: &LightingConfig,
    home: &CameraHome,
    quality: QualityPreset,
    disable: &crate::diagnostics::DisableSet,
) -> Entity {
    let exposure = Exposure {
        ev100: lighting.camera.ev100(),
    };

    let mut pan = PanOrbitCamera::default().with_initial_anchor_depth(home.distance as f64);
    pan.perspective.near_clip_limits = lighting.camera.near_m..lighting.camera.near_m;
    pan.zoom_limits.min_size_per_pixel = 1e-5;
    pan.zoom_limits.max_size_per_pixel = 0.1;

    let cam = commands
        .spawn((
            ViewCamera,
            Name::new("view_camera"),
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: lighting.camera.fov_deg.to_radians(),
                near: lighting.camera.near_m,
                far: lighting.camera.far_m,
                ..default()
            }),
            orbit_transform(home),
            // `Hdr` is added by `apply_quality` only for presets with bloom (decision 0006).
            Msaa::Off,
            if disable.tonemap {
                Tonemapping::None
            } else {
                Tonemapping::AgX
            },
            exposure,
            ColorGrading {
                global: ColorGradingGlobal {
                    post_saturation: if disable.tonemap {
                        1.0
                    } else {
                        lighting.camera.post_saturation
                    },
                    ..default()
                },
                ..default()
            },
            pan,
        ))
        .id();
    if disable.env {
        // A flat ambient light stands in for the studio so the scene is still lit.
        commands.insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 120.0,
            ..default()
        });
    } else {
        let env = images.add(crate::hdri::load_studio_cubemap(
            lighting.environment.cube_size.clamp(16, 1024),
        ));
        commands.entity(cam).insert(GeneratedEnvironmentMapLight {
            environment_map: env,
            intensity: lighting.environment.intensity,
            ..default()
        });
    }

    let dir = axl_to_bevy(lighting.key_light_direction_axl());
    commands.spawn((
        KeyLightMarker,
        Name::new("key_light"),
        DirectionalLight {
            color: srgb(lighting.key_light.color_srgb),
            illuminance: lighting.key_light.illuminance_lux,
            shadow_maps_enabled: !disable.shadows,
            // The default biases are tuned for metre-scale scenes; parts are centimetres.
            shadow_depth_bias: 0.0003,
            shadow_normal_bias: 0.4,
            ..default()
        },
        Transform::from_translation(Vec3::ZERO).looking_to(dir, Vec3::Y),
    ));

    commands.insert_resource(ActiveQuality(quality));
    commands.insert_resource(ClearColor(srgb(lighting.environment.backdrop_srgb)));
    cam
}

/// Adds the camera rig systems.
pub struct CameraRigPlugin {
    /// Leave mesh picking out (`--disable picking`).
    pub no_picking: bool,
}

impl Plugin for CameraRigPlugin {
    fn build(&self, app: &mut App) {
        if !self.no_picking {
            app.add_plugins(MeshPickingPlugin);
        }
        app.add_plugins((
            DefaultPanOrbitCameraPlugins,
            crate::camera_input::AxlCameraInputPlugin,
        ))
        .init_resource::<UiCapture>()
        .init_resource::<CameraMode>()
        .init_resource::<CameraHome>()
        .init_resource::<FocusTarget>()
        .init_resource::<CameraSettings>()
        .init_resource::<DirectionalLightShadowMap>()
        .add_systems(
            Update,
            (
                view_keys,
                apply_camera_mode,
                follow_target,
                apply_orbit_inversion,
                apply_quality,
            ),
        );
    }
}

fn view_keys(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<UiCapture>,
    mut mode: ResMut<CameraMode>,
    home: Res<CameraHome>,
    focus: Res<FocusTarget>,
    targets: Query<&GlobalTransform>,
    mut cams: Query<(&mut Transform, &mut PanOrbitCamera), With<ViewCamera>>,
) {
    if ui.keyboard {
        return;
    }
    let Ok((mut tf, mut pan)) = cams.single_mut() else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyV) {
        *mode = mode.next();
    }
    if keys.just_pressed(KeyCode::Home) {
        *tf = if *mode == CameraMode::TopDown {
            top_down_transform(home.focus, home.distance)
        } else {
            orbit_transform(&home)
        };
        pan.last_anchor_depth = -(home.distance as f64);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        // Centre on the focus target if there is one, keeping the viewing direction and distance.
        if let Some(target) = focus.0.and_then(|e| targets.get(e).ok()) {
            let dist = pan.last_anchor_depth.abs() as f32;
            let centre = target.translation();
            let back = tf.rotation * Vec3::Z;
            tf.translation = centre + back * dist;
        } else {
            *tf = orbit_transform(&home);
            pan.last_anchor_depth = -(home.distance as f64);
        }
    }
}

/// Switch the projection and controller options when the mode changes.
fn apply_camera_mode(
    mode: Res<CameraMode>,
    home: Res<CameraHome>,
    mut last: Local<Option<CameraMode>>,
    mut cams: Query<(&mut Transform, &mut Projection, &mut PanOrbitCamera), With<ViewCamera>>,
    lighting: Res<LightingConfig>,
) {
    if *last == Some(*mode) {
        return;
    }
    let first = last.is_none();
    *last = Some(*mode);
    if first && *mode == CameraMode::Orbit {
        return; // spawn_view_camera already set up the default orbit view
    }
    let Ok((mut tf, mut projection, mut pan)) = cams.single_mut() else {
        return;
    };
    match *mode {
        CameraMode::TopDown => {
            *projection = Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: home.distance,
                },
                ..OrthographicProjection::default_3d()
            });
            *tf = top_down_transform(home.focus, 1.5);
            pan.enabled_motion.orbit = false;
            // Keep the camera exactly 1.5 m above the floor; the controller would otherwise
            // push it away to a distance based on the projection scale.
            pan.orthographic.near_clip_limits = 1.5..1.5;
            pan.last_anchor_depth = -1.5;
        }
        CameraMode::Orbit | CameraMode::Follow => {
            *projection = Projection::Perspective(PerspectiveProjection {
                fov: lighting.camera.fov_deg.to_radians(),
                near: lighting.camera.near_m,
                far: lighting.camera.far_m,
                ..default()
            });
            pan.enabled_motion.orbit = true;
            *tf = orbit_transform(&home);
            pan.last_anchor_depth = -(home.distance as f64);
        }
    }
}

/// In follow mode, carry the camera along with the followed entity.
fn follow_target(
    mode: Res<CameraMode>,
    focus: Res<FocusTarget>,
    targets: Query<&GlobalTransform, Without<ViewCamera>>,
    mut last: Local<Option<Vec3>>,
    mut cams: Query<&mut Transform, With<ViewCamera>>,
) {
    let current = (*mode == CameraMode::Follow)
        .then(|| {
            focus
                .0
                .and_then(|e| targets.get(e).ok())
                .map(|t| t.translation())
        })
        .flatten();
    if let (Some(now), Some(prev), Ok(mut tf)) = (current, *last, cams.single_mut()) {
        tf.translation += now - prev;
    }
    *last = current;
}

fn apply_orbit_inversion(
    settings: Res<CameraSettings>,
    mut cams: Query<&mut PanOrbitCamera, With<ViewCamera>>,
) {
    if !settings.is_changed() {
        return;
    }
    for mut pan in &mut cams {
        let s = if settings.invert_orbit { -1.0 } else { 1.0 };
        pan.sensitivity.orbit = Vec2::splat(s);
    }
}

/// Apply the active quality preset to the camera, the key light and the shadow map.
#[allow(clippy::too_many_arguments)]
pub fn apply_quality(
    quality: Option<Res<ActiveQuality>>,
    disable: Res<crate::diagnostics::DisableSet>,
    lighting: Res<LightingConfig>,
    mut commands: Commands,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut last: Local<Option<QualityPreset>>,
    cams: Query<Entity, With<ViewCamera>>,
    mut lights: Query<(Entity, &mut DirectionalLight), With<KeyLightMarker>>,
) {
    let Some(quality) = quality else { return };
    let Ok(cam) = cams.single() else { return };
    if *last == Some(quality.0) {
        return;
    }
    *last = Some(quality.0);
    let mut q = quality.0.settings();
    if disable.taa && q.aa == AaMode::Taa {
        q.aa = AaMode::Fxaa;
    }
    if disable.ssao {
        q.ao = AoLevel::Off;
    }
    if disable.contact_shadows {
        q.contact_shadows = false;
    }
    if disable.bloom {
        q.bloom = false;
    }
    if disable.shadows {
        q.contact_shadows = false;
    }

    // Start from a clean slate for everything a preset can switch.
    let mut e = commands.entity(cam);
    e.remove::<(
        Fxaa,
        Smaa,
        TemporalAntiAliasing,
        TemporalJitter,
        MipBias,
        ScreenSpaceAmbientOcclusion,
        ContactShadows,
        Bloom,
    )>();
    e.remove::<(DepthPrepass, NormalPrepass, MotionVectorPrepass)>();

    match q.aa {
        AaMode::Fxaa => {
            e.insert(Fxaa::default());
        }
        AaMode::Smaa => {
            e.insert(Smaa {
                preset: SmaaPreset::High,
            });
        }
        AaMode::Taa => {
            e.insert(TemporalAntiAliasing::default());
        }
    }
    if q.bloom {
        e.insert(studio_bloom(lighting.bloom.intensity));
    }
    // Without bloom there is no need for a half-float target: an 8-bit one halves the memory
    // traffic, which is what limits integrated GPUs (tonemapping then happens in the shader).
    if q.bloom {
        e.insert(Hdr);
    } else {
        e.remove::<Hdr>();
    }
    if q.contact_shadows {
        // Distances are in metres; the defaults (30 cm rays) are far too long for small parts.
        e.insert(ContactShadows {
            linear_steps: 24,
            thickness: 0.005,
            length: 0.04,
        });
    }
    if q.ao != AoLevel::Off {
        let level = match q.ao {
            AoLevel::Low | AoLevel::Off => ScreenSpaceAmbientOcclusionQualityLevel::Low,
            AoLevel::Medium => ScreenSpaceAmbientOcclusionQualityLevel::Medium,
            AoLevel::High => ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
        };
        // Radius is in view-space metres; parts are centimetres across.
        e.insert(ScreenSpaceAmbientOcclusion {
            quality_level: level,
            radius: 0.012,
            constant_object_thickness: 0.004,
        });
    }
    e.insert(if q.shadows.soft && q.aa == AaMode::Taa {
        ShadowFilteringMethod::Temporal
    } else if q.shadows.soft {
        ShadowFilteringMethod::Gaussian
    } else {
        ShadowFilteringMethod::Hardware2x2
    });

    shadow_map.size = q.shadows.map_size as usize;
    for (light_entity, mut light) in &mut lights {
        light.contact_shadows_enabled = q.contact_shadows;
        light.shadow_maps_enabled = !disable.shadows;
        commands.entity(light_entity).insert(
            CascadeShadowConfigBuilder {
                num_cascades: q.shadows.cascades,
                minimum_distance: 0.02,
                maximum_distance: lighting.key_light.shadow_max_distance_m,
                first_cascade_far_bound: lighting.key_light.shadow_first_cascade_m,
                overlap_proportion: 0.2,
            }
            .build(),
        );
    }
}

/// Screen-space direction of each world axis for a camera with `camera_rotation`,
/// used to draw the corner XYZ gizmo.
///
/// Returns, for X, Y and Z (AXL axes), the 2D offset (right, up) of the axis tip on screen
/// for a unit-length axis, and its depth towards the viewer (positive is nearer).
pub fn gizmo_axes(camera_rotation: Quat) -> [(Vec2, f32); 3] {
    let inv = camera_rotation.inverse();
    let axl = [Vec3::X, Vec3::Y, Vec3::Z].map(|a| inv * axl_to_bevy(a));
    // In camera space +X is right, +Y is up, +Z points towards the viewer.
    axl.map(|v| (Vec2::new(v.x, v.y), v.z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycle_is_orbit_follow_top_down() {
        let m = CameraMode::Orbit;
        assert_eq!(m.next(), CameraMode::Follow);
        assert_eq!(m.next().next(), CameraMode::TopDown);
        assert_eq!(m.next().next().next(), CameraMode::Orbit);
    }

    #[test]
    fn orbit_position_matches_yaw_pitch() {
        let p = orbit_position(Vec3::ZERO, 0.0, 0.0, 2.0);
        assert!((p - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-6);
        let p = orbit_position(Vec3::ZERO, 90.0, 90.0, 1.0);
        assert!((p - Vec3::Z).length() < 1e-6);
        let p = orbit_position(Vec3::new(1.0, 1.0, 1.0), 180.0, 0.0, 1.0);
        assert!((p - Vec3::new(0.0, 1.0, 1.0)).length() < 1e-5);
    }

    #[test]
    fn orbit_transform_looks_at_the_focus_with_y_up() {
        let home = CameraHome::default();
        let tf = orbit_transform(&home);
        let to_focus = (axl_to_bevy(home.focus) - tf.translation).normalize();
        assert!((tf.forward().as_vec3() - to_focus).length() < 1e-5);
        // No roll: the camera's right vector is horizontal in the Bevy world.
        assert!(tf.right().y.abs() < 1e-5);
        assert!(tf.up().y > 0.0);
    }

    #[test]
    fn top_down_has_axl_y_up_the_screen() {
        let tf = top_down_transform(Vec3::new(0.2, 0.1, 0.0), 1.0);
        // Looking straight down.
        assert!((tf.forward().as_vec3() - Vec3::NEG_Y).length() < 1e-5);
        // Screen-up is AXL +Y, which is Bevy -Z; screen-right is AXL +X.
        assert!((tf.up().as_vec3() - Vec3::NEG_Z).length() < 1e-5);
        assert!((tf.right().as_vec3() - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn gizmo_axes_for_top_down_view() {
        let tf = top_down_transform(Vec3::ZERO, 1.0);
        let [(x, xz), (y, yz), (z, zz)] = gizmo_axes(tf.rotation);
        assert!((x - Vec2::X).length() < 1e-5, "{x}");
        assert!((y - Vec2::Y).length() < 1e-5, "{y}");
        assert!(z.length() < 1e-5);
        assert!(zz > 0.99, "Z points at the viewer from above");
        assert!(xz.abs() < 1e-5 && yz.abs() < 1e-5);
    }

    #[test]
    fn gizmo_axes_for_default_orbit_puts_z_up_on_screen() {
        let tf = orbit_transform(&CameraHome::default());
        let [_, _, (z, _)] = gizmo_axes(tf.rotation);
        assert!(z.y > 0.5 && z.x.abs() < 1e-4, "{z}");
    }
}
