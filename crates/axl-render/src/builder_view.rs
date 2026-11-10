//! The Builder screen's 3D side: the robot as meshes, the translucent ghost, highlights, stud dots
//! and the measure tool on part surfaces, and the mouse and keyboard that drive
//! [`axl_build::Builder`]. The egui panels live in `axl-app`.
//!
//! Frames: the builder works in the robot frame (millimetres). The robot is drawn so its lowest
//! point rests on the floor, so a robot frame point `p` is at AXL `(p.x, p.y, p.z - floor_z)` mm.

use std::collections::BTreeMap;

use axl_build::place::{Hit, Ray};
use axl_build::validate::Level;
use axl_build::{Builder, Orient, TurnAxis};
use axl_parts::types::{Axis, Surface};
use bevy::camera::prelude::*;
use bevy::camera_controller::pan_orbit_camera::prelude::*;
use bevy::prelude::*;

use crate::camera_rig::{
    ActiveQuality, CameraHome, FocusTarget, UiCapture, ViewCamera, orbit_transform,
};
use crate::frames::{AxlRoot, axl_to_bevy};
use crate::grid::{CursorOnFloor, GridSettings, Measure, SurfacePicking};
use crate::parts::PartLibrary;
use crate::scene::{PartMaterials, resolve_color};

const MM: f32 = 0.001;

/// Every entity the builder view has spawned.
type AnyBuilderPart = Or<(With<BuilderPart>, With<GhostPart>)>;

/// The builder and a few UI-facing facts about it.
#[derive(Resource)]
pub struct BuilderRes {
    /// The editor.
    pub b: Builder,
    /// The last message for the status line.
    pub status: String,
    /// Colour for newly picked parts (`None` uses each part's default).
    pub default_color: Option<String>,
}

/// Where the cursor is, for overlays and for the UI.
#[derive(Resource, Default)]
pub struct BuilderCursor {
    /// The cursor ray in the robot frame (mm).
    pub ray: Option<Ray>,
    /// The surface under it.
    pub hit: Option<Hit>,
}

/// A part entity of the robot.
#[derive(Component)]
pub struct BuilderPart(pub u32);

/// A ghost entity.
#[derive(Component)]
pub struct GhostPart;

/// Entities by part uid.
#[derive(Resource, Default)]
pub struct BuilderEntities(pub BTreeMap<u32, Entity>);

pub(crate) fn mat3(o: Orient) -> Mat3 {
    let m = o.matrix();
    Mat3::from_cols(
        Vec3::new(m[0][0] as f32, m[1][0] as f32, m[2][0] as f32),
        Vec3::new(m[0][1] as f32, m[1][1] as f32, m[2][1] as f32),
        Vec3::new(m[0][2] as f32, m[1][2] as f32, m[2][2] as f32),
    )
}

/// Transform (AXL frame, metres) of a part at robot-frame `pos` (0.1 mm) with `orient`.
pub fn part_transform(pos: [i32; 3], orient: Orient, floor_z_mm: f64) -> Transform {
    let t = Vec3::new(
        pos[0] as f32 * 1e-4,
        pos[1] as f32 * 1e-4,
        pos[2] as f32 * 1e-4 - floor_z_mm as f32 * MM,
    );
    Transform::from_translation(t).with_rotation(Quat::from_mat3(&mat3(orient)))
}

/// Robot-frame point (mm) to AXL metres.
fn to_axl(p: [f64; 3], floor_z: f64) -> Vec3 {
    Vec3::new(
        p[0] as f32 * MM,
        p[1] as f32 * MM,
        (p[2] - floor_z) as f32 * MM,
    )
}

fn bevy_pt(p: [f64; 3], floor_z: f64) -> Vec3 {
    axl_to_bevy(to_axl(p, floor_z))
}

/// The builder's plugin.
pub struct BuilderPlugin;

impl Plugin for BuilderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuilderCursor>()
            .init_resource::<BuilderEntities>()
            .init_resource::<BuilderSyncState>()
            .insert_resource(SurfacePicking)
            .add_systems(Startup, builder_setup)
            .add_systems(
                Update,
                (
                    builder_input.after(crate::grid::track_cursor),
                    (builder_sync, builder_ghost, builder_focus, builder_overlays)
                        .chain()
                        .after(builder_input),
                )
                    .run_if(crate::stage_swap::in_builder),
            )
            .add_systems(Update, builder_leave);
    }
}

fn builder_setup(mut commands: Commands, lib: Res<PartLibrary>, mut grid: ResMut<GridSettings>) {
    commands.insert_resource(BuilderRes {
        b: Builder::new(lib.catalog().clone()),
        status: "Pick a part on the left, then click on the robot to place it.".into(),
        default_color: None,
    });
    grid.stud_dots = true;
}

/// What the robot and ghost entities were last built from, so they are rebuilt only on change.
#[derive(Resource, Default)]
pub struct BuilderSyncState {
    rev: Option<u64>,
    ghost: Option<GhostSig>,
}

/// Leaving the builder takes its entities off the stage; coming back builds them again.
fn builder_leave(
    mut commands: Commands,
    kind: Res<crate::stage_swap::StageKind>,
    parts: Query<Entity, AnyBuilderPart>,
    mut ents: ResMut<BuilderEntities>,
    mut state: ResMut<BuilderSyncState>,
) {
    if !kind.is_changed() || *kind == crate::stage_swap::StageKind::Builder {
        return;
    }
    for e in &parts {
        commands.entity(e).despawn();
    }
    ents.0.clear();
    *state = BuilderSyncState::default();
}

#[allow(clippy::too_many_arguments)]
fn builder_input(
    mut res: Option<ResMut<BuilderRes>>,
    mut cursor: ResMut<BuilderCursor>,
    mut floor_cursor: ResMut<CursorOnFloor>,
    mut measure: ResMut<Measure>,
    grid: Res<GridSettings>,
    ui: Res<UiCapture>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewCamera>>,
    root: Query<&GlobalTransform, With<AxlRoot>>,
    mut first: Local<bool>,
    mut home: ResMut<CameraHome>,
    mut cams: Query<(&mut Transform, &mut PanOrbitCamera), With<ViewCamera>>,
) {
    let Some(res) = res.as_mut() else { return };
    if !*first {
        *first = true;
        *home = CameraHome {
            focus: Vec3::new(0.0, 0.0, 0.015),
            yaw_deg: -55.0,
            pitch_deg: 32.0,
            distance: 0.42,
        };
        if let Ok((mut tf, mut pan)) = cams.single_mut() {
            *tf = orbit_transform(&home);
            pan.last_anchor_depth = -(home.distance as f64);
        }
    }
    let floor_z = res.b.floor_z_mm();
    cursor.ray = None;
    cursor.hit = None;
    if !ui.pointer
        && let (Ok(window), Ok((camera, cam_tf)), Ok(root_tf)) =
            (windows.single(), cameras.single(), root.single())
        && let Some(pos) = window.cursor_position()
        && let Ok(ray) = camera.viewport_to_world(cam_tf, pos)
    {
        let inv = root_tf.affine().inverse();
        let o = inv.transform_point3(ray.origin);
        let d = inv.transform_vector3(*ray.direction);
        let r = Ray {
            origin: [
                o.x as f64 * 1000.0,
                o.y as f64 * 1000.0,
                o.z as f64 * 1000.0 + floor_z,
            ],
            dir: [d.x as f64, d.y as f64, d.z as f64],
        };
        cursor.hit = res.b.hit(&r);
        cursor.ray = Some(r);
    }
    probe(floor_z, &cameras, &windows, &root);
    // Stud dots on the floor only where the cursor really is on the floor.
    if cursor.hit.is_some_and(|h| h.uid.is_some()) {
        floor_cursor.0 = None;
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

    if !ui.keyboard {
        let BuilderRes { b, status, .. } = &mut **res;
        let mut say = |s: String| *status = s;
        if keys.just_pressed(KeyCode::KeyZ) && ctrl {
            let did = if shift { b.redo() } else { b.undo() };
            say(did
                .map(|l| format!("{} {l}", if shift { "Redid" } else { "Undid" }))
                .unwrap_or_else(|| "Nothing to undo.".into()));
        } else if keys.just_pressed(KeyCode::KeyY) && ctrl {
            if let Some(l) = b.redo() {
                say(format!("Redid {l}"));
            }
        } else if keys.just_pressed(KeyCode::KeyD) && ctrl {
            if let Err(e) = b.start_duplicate() {
                say(e.to_string());
            }
        } else if !ctrl {
            if keys.just_pressed(KeyCode::KeyR) && b.ghost.is_some() {
                b.rotate_ghost(TurnAxis::Z);
            }
            if keys.just_pressed(KeyCode::KeyT) && b.ghost.is_some() {
                b.rotate_ghost(TurnAxis::X);
            }
            if keys.just_pressed(KeyCode::KeyY) && b.ghost.is_some() {
                b.rotate_ghost(TurnAxis::Y);
            }
            if keys.just_pressed(KeyCode::KeyU) && b.ghost.is_some() {
                b.underneath = !b.underneath;
                say(if b.underneath {
                    "Aiming through the robot to its underside.".into()
                } else {
                    "Aiming at the top of the robot.".into()
                });
            }
            if keys.just_pressed(KeyCode::KeyG) && b.ghost.is_none() {
                match b.start_move() {
                    Ok(()) => say("Moving: click to put it down, Esc to cancel.".into()),
                    Err(e) => say(e.to_string()),
                }
            }
            if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
                match b.delete_selected() {
                    Ok(n) => say(format!(
                        "Deleted {n} part{}.",
                        if n == 1 { "" } else { "s" }
                    )),
                    Err(e) => say(e.to_string()),
                }
            }
            if keys.just_pressed(KeyCode::Escape) {
                if b.ghost.is_some() {
                    b.cancel();
                } else {
                    b.select(None, false);
                }
            }
        }
    }

    // Hover and click.
    if let Some(ray) = cursor.ray {
        if res.b.ghost.is_some() {
            res.b.hover(&ray);
        }
        if mouse.just_pressed(MouseButton::Left) && !alt {
            if grid.measure_active {
                if let Some(h) = cursor.hit {
                    measure.click3(to_axl(h.point, floor_z));
                }
            } else if res.b.ghost.is_some() {
                match res.b.click_place() {
                    Ok(u) => {
                        res.status = format!(
                            "Placed {} part{}.",
                            u.len(),
                            if u.len() == 1 { "" } else { "s" }
                        )
                    }
                    Err(e) => res.status = e.to_string(),
                }
            } else {
                let pick = res.b.pick(&ray);
                res.b.select(pick, shift);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn builder_sync(
    mut commands: Commands,
    res: Option<Res<BuilderRes>>,
    old: Query<Entity, With<BuilderPart>>,
    root: Query<Entity, With<AxlRoot>>,
    mut ents: ResMut<BuilderEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut parts: ResMut<PartMaterials>,
    mut lib: ResMut<PartLibrary>,
    quality: Res<ActiveQuality>,
    mut state: ResMut<BuilderSyncState>,
) {
    let last = &mut state.rev;
    let Some(res) = res else { return };
    let (Ok(root), true) = (root.single(), *last != Some(res.b.rev)) else {
        return;
    };
    *last = Some(res.b.rev);
    for e in &old {
        commands.entity(e).despawn();
    }
    ents.0.clear();
    let mut sb = crate::scene::ShapeBuilder {
        meshes: &mut meshes,
        materials: &mut std_mats,
        parts: &mut parts,
        lib: &mut lib,
        micro_on: quality.0.settings().micro_surface,
    };
    let fz = res.b.floor_z_mm();
    for p in &res.b.assembly.parts {
        let tf = part_transform(p.pos, p.orient, fz);
        if let Some(e) = sb.spawn_part(&mut commands, root, &p.part.0, &p.color.0, p.uid, tf) {
            commands.entity(e).insert(BuilderPart(p.uid));
            ents.0.insert(p.uid, e);
        }
    }
}

type GhostSig = (
    Vec<(String, String, u8, [i32; 3])>,
    [i32; 3],
    bool,
    bool,
    i64,
);

#[allow(clippy::too_many_arguments)]
fn builder_ghost(
    mut commands: Commands,
    res: Option<Res<BuilderRes>>,
    old: Query<Entity, With<GhostPart>>,
    root: Query<Entity, With<AxlRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut lib: ResMut<PartLibrary>,
    mut state: ResMut<BuilderSyncState>,
) {
    let last = &mut state.ghost;
    let Some(res) = res else { return };
    let Ok(root) = root.single() else { return };
    let sig: Option<GhostSig> = res.b.ghost.as_ref().filter(|g| g.placed).map(|g| {
        (
            g.items
                .iter()
                .map(|i| (i.part.0.clone(), i.color.0.clone(), i.orient.index(), i.rel))
                .collect(),
            g.anchor,
            g.valid,
            true,
            (res.b.floor_z_mm() * 10.0) as i64,
        )
    });
    if *last == sig {
        return;
    }
    for e in &old {
        commands.entity(e).despawn();
    }
    *last = sig.clone();
    let (Some(g), Some(_)) = (res.b.ghost.as_ref().filter(|g| g.placed), sig) else {
        return;
    };
    let fz = res.b.floor_z_mm();
    for it in &g.items {
        let pos = [0, 1, 2].map(|k| g.anchor[k] + it.rel[k]);
        let Some(cached) = lib.cached(&it.part.0, &mut meshes).cloned() else {
            continue;
        };
        let default_color = lib
            .catalog()
            .get(&it.part.0)
            .and_then(|d| d.colors.first().map(|c| c.0.clone()));
        let rgb = resolve_color(&it.color.0)
            .or_else(|| default_color.as_deref().and_then(resolve_color))
            .unwrap_or([200, 200, 200]);
        let parent = commands
            .spawn((
                GhostPart,
                part_transform(pos, it.orient, fz),
                Visibility::default(),
            ))
            .id();
        commands.entity(root).add_child(parent);
        for (_, surface, mesh) in &cached.slots {
            let base = match surface {
                Surface::Body => rgb,
                Surface::Dark => [0x1B, 0x2A, 0x34],
                Surface::Rubber => [0x1A, 0x1A, 0x1A],
                Surface::Steel => [0xBF, 0xBF, 0xBF],
            };
            let tint = if g.valid {
                Color::srgba_u8(base[0], base[1], base[2], 140)
            } else {
                Color::srgba(0.95, 0.15, 0.12, 0.55)
            };
            let mat = std_mats.add(StandardMaterial {
                base_color: tint,
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.4,
                ..default()
            });
            let child = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::default(),
                ))
                .id();
            commands.entity(parent).add_child(child);
        }
    }
}

fn builder_focus(
    res: Option<Res<BuilderRes>>,
    ents: Res<BuilderEntities>,
    mut focus: ResMut<FocusTarget>,
) {
    let Some(res) = res else { return };
    focus.0 = res.b.selection.first().and_then(|u| ents.0.get(u).copied());
}

fn line_box(g: &mut Gizmos, lo: [f64; 3], hi: [f64; 3], fz: f64, color: Color) {
    let c = |i: usize| -> Vec3 {
        bevy_pt(
            [
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ],
            fz,
        )
    };
    for i in 0..8usize {
        for bit in [1usize, 2, 4] {
            if i & bit == 0 {
                g.line(c(i), c(i | bit), color);
            }
        }
    }
}

fn builder_overlays(
    res: Option<Res<BuilderRes>>,
    cursor: Res<BuilderCursor>,
    measure: Res<Measure>,
    grid: Res<GridSettings>,
    mut g: Gizmos,
) {
    let Some(res) = res else { return };
    let b = &res.b;
    let fz = b.floor_z_mm();
    let bounds = |uid: u32| -> Option<([f64; 3], [f64; 3])> {
        let (_, ps) = b.prims().iter().find(|(u, _)| *u == uid)?;
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for (_, p) in ps {
            let (l, h) = p.aabb();
            for k in 0..3 {
                lo[k] = lo[k].min(l[k]);
                hi[k] = hi[k].max(h[k]);
            }
        }
        Some((lo, hi))
    };
    // Problems: errors red, warnings amber.
    for issue in b.issues() {
        let color = match issue.level {
            Level::Error => Color::srgb(1.0, 0.15, 0.1),
            Level::Warning => Color::srgb(1.0, 0.75, 0.1),
        };
        for u in &issue.parts {
            if let Some((lo, hi)) = bounds(*u) {
                line_box(&mut g, lo, hi, fz, color);
            }
        }
    }
    for u in &b.selection {
        if let Some((lo, hi)) = bounds(*u) {
            let e = 0.6;
            line_box(
                &mut g,
                lo.map(|v| v - e),
                hi.map(|v| v + e),
                fz,
                Color::srgb(1.0, 0.54, 0.09),
            );
        }
    }
    // Snap marker: the free connector the ghost joins.
    if let Some(snap) = b.ghost.as_ref().and_then(|gh| gh.snap)
        && let Some(w) = b.index().conns.iter().find(|w| w.r == snap.target)
    {
        let p = w.pos.map(|v| v as f64 * 0.1);
        let c = Color::srgb(0.2, 1.0, 0.4);
        for k in 0..3 {
            let mut a = p;
            let mut z = p;
            a[k] -= 2.0;
            z[k] += 2.0;
            g.line(bevy_pt(a, fz), bevy_pt(z, fz), c);
        }
    }
    // Stud dots on the surface under the cursor: the stud and hole positions on that face.
    if grid.stud_dots
        && let Some(hit) = cursor.hit.filter(|h| h.uid.is_some())
    {
        let n_axis = hit.axis;
        let normal = {
            let mut n = Vec3::ZERO;
            n[n_axis] = if hit.positive { 1.0 } else { -1.0 };
            axl_to_bevy(n)
        };
        let rot = Quat::from_rotation_arc(Vec3::Z, normal);
        for w in &b.index().conns {
            let p = w.pos.map(|v| v as f64 * 0.1);
            let on_face = (p[n_axis] - hit.point[n_axis]).abs() < 0.6;
            let along = matches!(
                (w.axis, n_axis),
                (Axis::PosX | Axis::NegX, 0)
                    | (Axis::PosY | Axis::NegY, 1)
                    | (Axis::PosZ | Axis::NegZ, 2)
            );
            let d = (0..3)
                .map(|k| (p[k] - hit.point[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            if on_face && along && d < 64.0 {
                let fade = (1.0 - d / 64.0) as f32;
                g.circle(
                    Isometry3d::new(bevy_pt(p, fz), rot),
                    1.2 * MM * 1.0,
                    Color::srgba(1.0, 1.0, 1.0, 0.15 + 0.7 * fade),
                );
            }
        }
    }
    // Measure on surfaces.
    let pts = measure.points3();
    if !measure.all_on_floor() || (grid.measure_active && !pts.is_empty()) {
        let col = Color::srgb(1.0, 0.54, 0.09);
        for p in &pts {
            let c = axl_to_bevy(*p);
            g.sphere(Isometry3d::from_translation(c), 1.6 * MM, col);
        }
        if let [a, b2] = pts.as_slice() {
            g.line(axl_to_bevy(*a), axl_to_bevy(*b2), col);
        }
    }
}

/// Test aid for scripted runs: when `AXL_BUILDER_PROBE=<path>` is set, the points listed in
/// `<path>` (robot frame, mm, one `x y z` per line) are projected to window pixels and written to
/// `<path>.out`, followed by the camera position. A script uses it to aim real mouse events.
fn probe(
    floor_z: f64,
    cameras: &Query<(&Camera, &GlobalTransform), With<ViewCamera>>,
    windows: &Query<&Window, With<bevy::window::PrimaryWindow>>,
    root: &Query<&GlobalTransform, With<AxlRoot>>,
) {
    let Some(path) = std::env::var_os("AXL_BUILDER_PROBE") else {
        return;
    };
    let (Ok((camera, cam_tf)), Ok(_), Ok(_)) = (cameras.single(), windows.single(), root.single())
    else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let mut out = String::new();
    for line in text.lines() {
        let v: Vec<f64> = line
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if let [x, y, z] = v[..]
            && let Ok(px) = camera.world_to_viewport(cam_tf, bevy_pt([x, y, z], floor_z))
        {
            out.push_str(&format!("{:.0} {:.0}\n", px.x, px.y));
        } else {
            out.push_str("none\n");
        }
    }
    let c = cam_tf.translation();
    let a = crate::frames::bevy_to_axl(c);
    out.push_str(&format!(
        "cam {:.0} {:.0} {:.0}\n",
        a.x * 1000.0,
        a.y * 1000.0,
        a.z * 1000.0 + floor_z as f32
    ));
    let mut p = path.clone();
    p.push(".out");
    let _ = std::fs::write(p, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_transform_places_in_metres_and_lifts_by_the_floor() {
        let t = part_transform([400, -800, 0], Orient::IDENTITY, -40.0);
        assert!((t.translation - Vec3::new(0.04, -0.08, 0.04)).length() < 1e-6);
        let turned = part_transform([0; 3], Orient::IDENTITY.turned(TurnAxis::Z), 0.0);
        assert!((turned.rotation * Vec3::X - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn measure_on_a_surface_uses_the_3d_distance() {
        let mut m = Measure::default();
        m.click3(Vec3::new(0.0, 0.0, 0.0));
        m.click3(Vec3::new(0.0, 0.0, 0.0192));
        assert!((m.distance_mm().unwrap() - 19.2).abs() < 1e-3);
        assert!(!m.all_on_floor());
    }
}
