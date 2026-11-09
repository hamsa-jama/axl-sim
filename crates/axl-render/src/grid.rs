//! The grid floor: an effectively infinite 200 m quad whose material draws printed-ink
//! grid lines in a shader (`shaders/grid_floor.wesl`), plus the builder's stud dots and
//! measure tool.

use bevy::asset::{AssetPath, embedded_asset};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use crate::frames::AxlRoot;
use crate::grid_math::{
    FADE_FAR_M, FADE_NEAR_M, GridUnits, MIN_LINE_SPACING_PX, OPACITY_BUILDER, OPACITY_SIMULATE,
    spacing_in_pixels,
};
use crate::lighting::{LightingConfig, srgb};

/// Edge length of the floor quad in metres.
pub const FLOOR_SIZE_M: f32 = 200.0;

/// Where the app is, for choosing the default grid opacity.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Building a robot (grid at 60% by default). Also used by the look-dev scene.
    #[default]
    Builder,
    /// Running a simulation (grid at 25% by default).
    Simulate,
}

/// User-facing grid options.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct GridSettings {
    /// Stud or metric lines.
    pub units: GridUnits,
    /// Opacity while building, 0..=1.
    pub opacity_builder: f32,
    /// Opacity while simulating, 0..=1.
    pub opacity_simulate: f32,
    /// Show stud-position dots under the cursor (builder extra).
    pub stud_dots: bool,
    /// The measure tool is active: clicks place measuring points.
    pub measure_active: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            units: GridUnits::Studs,
            opacity_builder: OPACITY_BUILDER,
            opacity_simulate: OPACITY_SIMULATE,
            stud_dots: true,
            measure_active: false,
        }
    }
}

impl GridSettings {
    /// The opacity to draw with in `mode`.
    pub fn opacity(&self, mode: ViewMode) -> f32 {
        match mode {
            ViewMode::Builder => self.opacity_builder,
            ViewMode::Simulate => self.opacity_simulate,
        }
        .clamp(0.0, 1.0)
    }
}

/// Uniform block read by the grid shader. Field meanings are documented in the shader.
#[derive(Clone, Copy, Debug, ShaderType)]
pub struct GridParams {
    spacing: Vec4,
    ink: Vec4,
    fade: Vec4,
    cursor: Vec4,
    measure: Vec4,
    measure_count: Vec4,
}

impl GridParams {
    /// Build the uniform for the given settings.
    pub fn new(
        units: GridUnits,
        opacity: f32,
        ink: [f32; 3],
        cursor: Option<Vec2>,
        measure: &Measure,
    ) -> Self {
        let [minor, major, bold] = units.spacings();
        let c = cursor.unwrap_or(Vec2::ZERO);
        let p0 = measure.points.first().copied().unwrap_or(Vec2::ZERO);
        let p1 = measure.points.get(1).copied().unwrap_or(Vec2::ZERO);
        Self {
            spacing: Vec4::new(minor, major, bold, 1.0),
            ink: Vec4::new(ink[0], ink[1], ink[2], opacity.clamp(0.0, 1.0)),
            fade: Vec4::new(FADE_NEAR_M, FADE_FAR_M, MIN_LINE_SPACING_PX, 2.0),
            // Dots reach 12 studs around the cursor.
            cursor: Vec4::new(
                c.x,
                c.y,
                minor * 12.0,
                if cursor.is_some() { 1.0 } else { 0.0 },
            ),
            measure: Vec4::new(p0.x, p0.y, p1.x, p1.y),
            measure_count: Vec4::new(measure.points.len() as f32, 0.0, 0.0, 0.0),
        }
    }
}

/// The extension that adds grid lines to `StandardMaterial`.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct GridExt {
    /// Shader uniform (binding 100 keeps clear of the base material's bindings 0 to 99).
    #[uniform(100)]
    pub params: GridParams,
}

impl MaterialExtension for GridExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(AssetPath::from(
            "embedded://axl_render/shaders/grid_floor.wesl",
        ))
    }
}

/// The grid floor material type.
pub type GridMaterial = ExtendedMaterial<StandardMaterial, GridExt>;

/// Marks the floor entity.
#[derive(Component)]
pub struct GridFloor;

/// Points picked with the measure tool, in AXL metres.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Measure {
    /// Zero, one or two points.
    pub points: Vec<Vec2>,
}

impl Measure {
    /// Add a point; a third click starts a new measurement.
    pub fn click(&mut self, p: Vec2) {
        if self.points.len() >= 2 {
            self.points.clear();
        }
        self.points.push(p);
    }

    /// Distance between the two points in millimetres, once both are set.
    pub fn distance_mm(&self) -> Option<f32> {
        match self.points.as_slice() {
            [a, b] => Some(a.distance(*b) * 1000.0),
            _ => None,
        }
    }

    /// The same distance in studs (8 mm each).
    pub fn distance_studs(&self) -> Option<f32> {
        self.distance_mm().map(|mm| mm / 8.0)
    }
}

/// The point on the floor (z = 0) under the mouse cursor, in AXL metres.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorOnFloor(pub Option<Vec2>);

/// Intersect a ray (AXL frame) with the floor plane z = 0.
pub fn ray_floor_hit(origin: Vec3, dir: Vec3) -> Option<Vec2> {
    if dir.z.abs() < 1e-9 {
        return None;
    }
    let t = -origin.z / dir.z;
    if t <= 0.0 {
        return None;
    }
    let p = origin + dir * t;
    Some(Vec2::new(p.x, p.y))
}

/// Builds the floor quad: lies in the AXL XY plane, normal +Z, UVs are metres.
pub fn floor_mesh(size: f32) -> Mesh {
    let h = size * 0.5;
    let positions = vec![[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0]];
    let normals = vec![[0.0, 0.0, 1.0]; 4];
    let uvs = vec![[-h, -h], [h, -h], [h, h], [-h, h]];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// The floor's shared material handle.
#[derive(Resource)]
pub struct FloorMaterial(pub Handle<GridMaterial>);

/// Adds the grid material, floor resources and the update systems.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/grid_floor.wesl");
        app.add_plugins(MaterialPlugin::<GridMaterial>::default())
            .init_resource::<GridSettings>()
            .init_resource::<ViewMode>()
            .init_resource::<Measure>()
            .init_resource::<CursorOnFloor>()
            .add_systems(
                Update,
                (track_cursor, measure_clicks, update_grid_uniforms).chain(),
            );
    }
}

/// Spawn the floor under `root`. Returns the entity.
pub fn spawn_floor(
    commands: &mut Commands,
    root: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<GridMaterial>,
    lighting: &LightingConfig,
    settings: &GridSettings,
) -> Entity {
    let ink = [
        lighting.floor.minor_ink,
        lighting.floor.major_ink,
        lighting.floor.bold_ink,
    ];
    let mat = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: srgb(lighting.floor.base_srgb),
            perceptual_roughness: lighting.floor.roughness,
            reflectance: lighting.floor.reflectance,
            metallic: 0.0,
            ..default()
        },
        extension: GridExt {
            params: GridParams::new(
                settings.units,
                settings.opacity(ViewMode::Builder),
                ink,
                None,
                &Measure::default(),
            ),
        },
    });
    commands.insert_resource(FloorMaterial(mat.clone()));
    let floor = commands
        .spawn((
            GridFloor,
            Name::new("grid_floor"),
            Mesh3d(meshes.add(floor_mesh(FLOOR_SIZE_M))),
            MeshMaterial3d(mat),
            Transform::default(),
            NotShadowCaster,
        ))
        .id();
    commands.entity(root).add_child(floor);
    floor
}

fn track_cursor(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<crate::camera_rig::ViewCamera>>,
    root: Query<&GlobalTransform, With<AxlRoot>>,
    ui: Res<crate::camera_rig::UiCapture>,
    mut cursor: ResMut<CursorOnFloor>,
) {
    cursor.0 = None;
    if ui.pointer {
        return;
    }
    let (Ok(window), Ok((camera, cam_tf)), Ok(root_tf)) =
        (windows.single(), cameras.single(), root.single())
    else {
        return;
    };
    let Some(pos) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tf, pos) else {
        return;
    };
    // Bring the ray into the AXL frame by undoing the root transform.
    let inv = root_tf.affine().inverse();
    let origin = inv.transform_point3(ray.origin);
    let dir = inv.transform_vector3(*ray.direction);
    cursor.0 = ray_floor_hit(origin, dir);
}

fn measure_clicks(
    settings: Res<GridSettings>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    cursor: Res<CursorOnFloor>,
    mut measure: ResMut<Measure>,
) {
    if !settings.measure_active {
        if !measure.points.is_empty() {
            measure.points.clear();
        }
        return;
    }
    // Alt + left-drag orbits the camera, so Alt-clicks never place points.
    if keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight) {
        return;
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(p) = cursor.0
    {
        measure.click(p);
    }
}

fn update_grid_uniforms(
    settings: Res<GridSettings>,
    mode: Res<ViewMode>,
    cursor: Res<CursorOnFloor>,
    measure: Res<Measure>,
    lighting: Res<LightingConfig>,
    floor: Option<Res<FloorMaterial>>,
    mut materials: ResMut<Assets<GridMaterial>>,
) {
    let Some(floor) = floor else { return };
    let Some(mut mat) = materials.get_mut(&floor.0) else {
        return;
    };
    let ink = [
        lighting.floor.minor_ink,
        lighting.floor.major_ink,
        lighting.floor.bold_ink,
    ];
    let dots = settings.stud_dots.then_some(cursor.0).flatten();
    mat.extension.params =
        GridParams::new(settings.units, settings.opacity(*mode), ink, dots, &measure);
}

/// Pixels between minor lines for a camera at `distance_m` looking straight down with the
/// given vertical field of view and viewport height. Used by tests and the UI hint.
pub fn minor_spacing_px(
    units: GridUnits,
    distance_m: f32,
    fov_y_rad: f32,
    viewport_h_px: f32,
) -> f32 {
    let metres_per_pixel = 2.0 * distance_m * (fov_y_rad * 0.5).tan() / viewport_h_px;
    spacing_in_pixels(units.spacings()[0], metres_per_pixel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_math::spacing_fade;

    #[test]
    fn default_opacity_by_mode() {
        let s = GridSettings::default();
        assert_eq!(s.opacity(ViewMode::Builder), 0.60);
        assert_eq!(s.opacity(ViewMode::Simulate), 0.25);
    }

    #[test]
    fn opacity_is_clamped() {
        let s = GridSettings {
            opacity_builder: 3.0,
            opacity_simulate: -1.0,
            ..default()
        };
        assert_eq!(s.opacity(ViewMode::Builder), 1.0);
        assert_eq!(s.opacity(ViewMode::Simulate), 0.0);
    }

    #[test]
    fn floor_hit_straight_down() {
        let p = ray_floor_hit(Vec3::new(0.1, 0.2, 1.0), Vec3::NEG_Z).unwrap();
        assert!((p - Vec2::new(0.1, 0.2)).length() < 1e-6);
    }

    #[test]
    fn floor_hit_angled_and_misses() {
        let p = ray_floor_hit(Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, -1.0)).unwrap();
        assert!((p.x - 1.0).abs() < 1e-6 && p.y.abs() < 1e-6);
        // Looking up or parallel to the floor never hits it.
        assert!(ray_floor_hit(Vec3::new(0.0, 0.0, 1.0), Vec3::Z).is_none());
        assert!(ray_floor_hit(Vec3::new(0.0, 0.0, 1.0), Vec3::X).is_none());
    }

    #[test]
    fn measure_reports_mm_and_studs_and_restarts() {
        let mut m = Measure::default();
        assert!(m.distance_mm().is_none());
        m.click(Vec2::new(0.0, 0.0));
        assert!(m.distance_mm().is_none());
        m.click(Vec2::new(0.048, 0.0));
        assert!((m.distance_mm().unwrap() - 48.0).abs() < 1e-3);
        assert!((m.distance_studs().unwrap() - 6.0).abs() < 1e-3);
        m.click(Vec2::new(1.0, 1.0));
        assert_eq!(m.points.len(), 1);
    }

    #[test]
    fn floor_mesh_uvs_are_metres_in_the_xy_plane() {
        let mesh = floor_mesh(FLOOR_SIZE_M);
        let pos = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .unwrap();
        let uv = match mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x2(v) => v.clone(),
            _ => panic!("uv format"),
        };
        for (p, t) in pos.iter().zip(uv.iter()) {
            assert_eq!(p[2], 0.0);
            assert_eq!([p[0], p[1]], *t);
        }
        assert_eq!(pos[2][0], 100.0);
    }

    #[test]
    fn grid_params_pack_units_and_opacity() {
        let p = GridParams::new(
            GridUnits::Metric,
            0.4,
            [0.3, 0.4, 0.5],
            Some(Vec2::new(1.0, 2.0)),
            &Measure::default(),
        );
        assert_eq!(p.spacing.truncate(), Vec3::new(0.01, 0.1, 1.0));
        assert_eq!(p.ink.w, 0.4);
        assert_eq!(p.cursor.w, 1.0);
        let off = GridParams::new(GridUnits::Studs, 2.0, [0.0; 3], None, &Measure::default());
        assert_eq!(off.ink.w, 1.0);
        assert_eq!(off.cursor.w, 0.0);
    }

    #[test]
    fn stud_lines_vanish_when_the_camera_is_far_overhead() {
        // 35 degree FOV, 1080 px tall. Close up, studs are well separated.
        let fov = 35f32.to_radians();
        let near = minor_spacing_px(GridUnits::Studs, 0.1, fov, 1080.0);
        let far = minor_spacing_px(GridUnits::Studs, 5.0, fov, 1080.0);
        assert!(spacing_fade(near) == 1.0, "{near}");
        assert!(spacing_fade(far) == 0.0, "{far}");
    }
}
