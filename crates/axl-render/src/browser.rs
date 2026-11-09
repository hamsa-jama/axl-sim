//! The debug part browser: shows the part catalog on the studio floor, either as a contact
//! sheet of every part that matches a filter, or one part on its own with its connectors and
//! collision shapes drawn over it. The egui side panel lives in `axl-app`.

use axl_parts::types::{Axis, CollisionPrim, ConnKind, PartDef};
use bevy::prelude::*;

use crate::camera_rig::{ActiveQuality, CameraHome, FocusTarget};
use crate::frames::{AxlRoot, axl_to_bevy};
use crate::parts::{PartLibrary, display_rotation};
use crate::scene::{PartMaterials, ShapeBuilder};

const MM: f32 = 0.001;

/// What the browser shows. Change it and the view rebuilds.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct BrowserState {
    /// Text filter on part id, name or category (empty or `all` shows every part).
    pub query: String,
    /// A single part to isolate, or `None` for the contact sheet.
    pub selected: Option<String>,
    /// Colour id for every shown part (`None` uses each part's default).
    pub color: Option<String>,
    /// Draw connectors on the isolated part.
    pub show_connectors: bool,
    /// Draw collision shapes on the isolated part.
    pub show_collision: bool,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            query: String::new(),
            selected: None,
            color: None,
            show_connectors: false,
            show_collision: false,
        }
    }
}

/// Facts about one part, for the side panel.
#[derive(Clone, Debug, PartialEq)]
pub struct PartStats {
    /// Triangles over all slots.
    pub triangles: usize,
    /// Size (x, y, z) in millimetres.
    pub size_mm: [f32; 3],
    /// Analytic solid volume in mm3, when the mass is computed.
    pub volume_mm3: Option<f64>,
    /// Total mass in grams.
    pub mass_g: f64,
    /// Connector count over all bodies.
    pub connectors: usize,
    /// Collision primitive count over all bodies.
    pub collisions: usize,
}

impl PartLibrary {
    /// Stats for the side panel; uploads the part's meshes if needed.
    pub fn stats(&mut self, id: &str, meshes: &mut Assets<Mesh>) -> Option<PartStats> {
        let (lo, hi, triangles) = {
            let c = self.cached(id, meshes)?;
            (c.bounds.0, c.bounds.1, c.triangles)
        };
        let def = self.catalog().get(id)?;
        Some(PartStats {
            triangles,
            size_mm: [(hi[0] - lo[0]) * 1e3, (hi[1] - lo[1]) * 1e3, (hi[2] - lo[2]) * 1e3],
            volume_mm3: def.solid_volume_m3().map(|v| v * 1e9),
            mass_g: def.bodies.iter().map(|b| b.mass_kg).sum::<f64>() * 1e3,
            connectors: def.bodies.iter().map(|b| b.connectors.len()).sum(),
            collisions: def.bodies.iter().map(|b| b.collision.len()).sum(),
        })
    }
}

/// True if `def` matches the browser filter.
pub fn matches_query(def: &PartDef, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() || q == "all" {
        return true;
    }
    let cat = format!("{:?}", def.category).to_lowercase();
    def.id.0.contains(&q) || def.name.to_lowercase().contains(&q) || cat == q || cat.contains(&q)
}

/// Flow layout of boxes (width, depth) into rows of at most `max_width`, starting a new row
/// when `break_before[i]` is set. Returns the centre of each box and the total (width, depth),
/// with the layout centred on the origin.
pub fn layout_rows(
    sizes: &[(f32, f32)],
    break_before: &[bool],
    max_width: f32,
    gap: f32,
) -> (Vec<Vec2>, Vec2) {
    // First pass: assign rows.
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut x = 0.0;
    for (i, &(w, _)) in sizes.iter().enumerate() {
        let new_row = rows.is_empty() || break_before.get(i).copied().unwrap_or(false) || (x + w > max_width && x > 0.0);
        if new_row {
            rows.push(Vec::new());
            x = 0.0;
        }
        rows.last_mut().map(|r| r.push(i));
        x += w + gap;
    }
    let row_w: Vec<f32> = rows
        .iter()
        .map(|r| r.iter().map(|&i| sizes[i].0).sum::<f32>() + gap * (r.len().saturating_sub(1)) as f32)
        .collect();
    let row_d: Vec<f32> = rows
        .iter()
        .map(|r| r.iter().map(|&i| sizes[i].1).fold(0.0, f32::max))
        .collect();
    let total_w = row_w.iter().cloned().fold(0.0, f32::max);
    let total_d = row_d.iter().sum::<f32>() + gap * (rows.len().saturating_sub(1)) as f32;
    let mut out = vec![Vec2::ZERO; sizes.len()];
    let mut y = total_d / 2.0;
    for (r, row) in rows.iter().enumerate() {
        let mut x = -row_w[r] / 2.0;
        for &i in row {
            out[i] = Vec2::new(x + sizes[i].0 / 2.0, y - row_d[r] / 2.0);
            x += sizes[i].0 + gap;
        }
        y -= row_d[r] + gap;
    }
    (out, Vec2::new(total_w, total_d))
}

/// Camera that frames a layout of `w` x `d` metres, `h` tall, from above at 55 degrees.
pub fn frame_layout(w: f32, d: f32, h: f32) -> CameraHome {
    let pitch: f32 = 55.0;
    let (vt, ht) = (17.5f32.to_radians().tan(), (17.5f32.to_radians().tan() * 1.6));
    let need_w = w * 1.08 / (2.0 * ht);
    let need_h = (d * pitch.to_radians().sin() + h * pitch.to_radians().cos()) * 1.12 / (2.0 * vt);
    CameraHome {
        focus: Vec3::new(0.0, 0.0, h * 0.25),
        yaw_deg: -90.0,
        pitch_deg: pitch,
        distance: need_w.max(need_h).max(0.12),
    }
}

/// Marks entities the browser spawned, so they can be replaced.
#[derive(Component)]
pub struct BrowserEntity;

/// The isolated part and where it sits (AXL frame), for the overlay drawing.
#[derive(Resource, Default)]
pub struct BrowserView {
    /// Part id and its placement.
    pub part: Option<(String, Transform)>,
}

/// Rotated bounds (min, max) of a part's bounding box under `rot`.
fn rotated_bounds(b: ([f32; 3], [f32; 3]), rot: Quat) -> (Vec3, Vec3) {
    let (lo, hi) = (Vec3::from(b.0), Vec3::from(b.1));
    let mut mn = Vec3::splat(f32::MAX);
    let mut mx = Vec3::splat(f32::MIN);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let p = rot * c;
        mn = mn.min(p);
        mx = mx.max(p);
    }
    (mn, mx)
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    state: Res<BrowserState>,
    mut commands: Commands,
    old: Query<Entity, With<BrowserEntity>>,
    root: Query<Entity, With<AxlRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut parts: ResMut<PartMaterials>,
    mut lib: ResMut<PartLibrary>,
    quality: Res<ActiveQuality>,
    mut home: ResMut<CameraHome>,
    mut focus: ResMut<FocusTarget>,
    mut view: ResMut<BrowserView>,
) {
    if !state.is_changed() {
        return;
    }
    let Ok(root) = root.single() else {
        return;
    };
    for e in &old {
        commands.entity(e).despawn();
    }
    let ids: Vec<String> = match &state.selected {
        Some(id) => vec![id.clone()],
        None => lib
            .catalog()
            .iter()
            .filter(|d| matches_query(d, &state.query))
            .map(|d| d.id.0.clone())
            .collect(),
    };
    // Display transform for each part: rotated bounds, sitting on the floor.
    struct Entry {
        id: String,
        rot: Quat,
        min: Vec3,
        max: Vec3,
        cat: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for id in &ids {
        let Some(def) = lib.catalog().get(id).cloned() else {
            continue;
        };
        let Some(b) = lib.cached(id, &mut meshes).map(|c| c.bounds) else {
            continue;
        };
        let rot = display_rotation(&def);
        let (min, max) = rotated_bounds(b, rot);
        entries.push(Entry { id: id.clone(), rot, min, max, cat: format!("{:?}", def.category) });
    }
    let sizes: Vec<(f32, f32)> = entries.iter().map(|e| (e.max.x - e.min.x, e.max.y - e.min.y)).collect();
    let breaks: Vec<bool> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| i > 0 && entries[i - 1].cat != e.cat)
        .collect();
    let (centres, total) = layout_rows(&sizes, &breaks, 0.62, 0.008);
    let height = entries.iter().map(|e| e.max.z - e.min.z).fold(0.0, f32::max);
    let mut builder = ShapeBuilder {
        meshes: &mut meshes,
        materials: &mut std_mats,
        parts: &mut parts,
        lib: &mut lib,
        micro_on: quality.0.settings().micro_surface,
    };
    view.part = None;
    let single = entries.len() == 1 && state.selected.is_some();
    let color = state.color.clone().unwrap_or_default();
    for (i, e) in entries.iter().enumerate() {
        let c = if single { Vec2::ZERO } else { centres[i] };
        let t = Vec3::new(c.x - (e.min.x + e.max.x) / 2.0, c.y - (e.min.y + e.max.y) / 2.0, -e.min.z);
        let tf = Transform::from_translation(t).with_rotation(e.rot);
        if let Some(ent) = builder.spawn_part(&mut commands, root, &e.id, &color, i as u32 + 1, tf) {
            commands.entity(ent).insert(BrowserEntity);
        }
        if single {
            view.part = Some((e.id.clone(), tf));
        }
    }
    focus.0 = None;
    *home = if single {
        let (w, d) = sizes[0];
        let mut h = frame_layout(w.max(0.02), d.max(0.02), height);
        h.distance = (h.distance * 1.35).max(0.1);
        h.yaw_deg = -50.0;
        h
    } else {
        frame_layout(total.x.max(0.05), total.y.max(0.05), height)
    };
}

fn axis_color(kind: ConnKind) -> Color {
    match kind {
        ConnKind::Stud => Color::srgb(0.2, 0.9, 0.3),
        ConnKind::AntiStud => Color::srgb(0.2, 0.8, 0.95),
        ConnKind::RoundHole => Color::srgb(1.0, 0.9, 0.1),
        ConnKind::AxleHole => Color::srgb(1.0, 0.5, 0.1),
        ConnKind::PinShaft { .. } => Color::srgb(0.95, 0.3, 0.9),
        ConnKind::AxleShaft { .. } => Color::srgb(0.6, 0.4, 1.0),
    }
}

fn axis_vec(a: Axis) -> Vec3 {
    let u = a.unit();
    Vec3::new(u[0] as f32, u[1] as f32, u[2] as f32)
}

fn overlays(state: Res<BrowserState>, view: Res<BrowserView>, lib: Res<PartLibrary>, mut gizmos: Gizmos) {
    let Some((id, tf)) = &view.part else {
        return;
    };
    let Some(def) = lib.catalog().get(id) else {
        return;
    };
    let w = |p: Vec3| axl_to_bevy(tf.transform_point(p));
    if state.show_connectors {
        for b in &def.bodies {
            for c in &b.connectors {
                let p = Vec3::new(c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32) * 0.1 * MM;
                let col = axis_color(c.kind);
                let r = 0.0012;
                gizmos.line(w(p - Vec3::X * r), w(p + Vec3::X * r), col);
                gizmos.line(w(p - Vec3::Y * r), w(p + Vec3::Y * r), col);
                gizmos.line(w(p - Vec3::Z * r), w(p + Vec3::Z * r), col);
                gizmos.line(w(p), w(p + axis_vec(c.axis) * 0.004), col);
            }
        }
    }
    if state.show_collision {
        let col = Color::srgb(1.0, 0.2, 0.2);
        for b in &def.bodies {
            for prim in &b.collision {
                match prim {
                    CollisionPrim::Box { center, half } => {
                        let (c, h) = (
                            Vec3::new(center[0] as f32, center[1] as f32, center[2] as f32),
                            Vec3::new(half[0] as f32, half[1] as f32, half[2] as f32),
                        );
                        let corner = |i: usize| c + h * Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
                        for i in 0..8usize {
                            for bit in [1usize, 2, 4] {
                                if i & bit == 0 {
                                    gizmos.line(w(corner(i)), w(corner(i | bit)), col);
                                }
                            }
                        }
                    }
                    CollisionPrim::Cylinder { center, axis, radius, half_len } => {
                        let c = Vec3::new(center[0] as f32, center[1] as f32, center[2] as f32);
                        let a = axis_vec(*axis).abs();
                        let (u, v) = if a.x > 0.5 { (Vec3::Y, Vec3::Z) } else if a.y > 0.5 { (Vec3::X, Vec3::Z) } else { (Vec3::X, Vec3::Y) };
                        let (r, hl) = (*radius as f32, *half_len as f32);
                        let n = 32;
                        for end in [-1.0f32, 1.0] {
                            for k in 0..n {
                                let (a0, a1) = (std::f32::consts::TAU * k as f32 / n as f32, std::f32::consts::TAU * (k + 1) as f32 / n as f32);
                                let p = |t: f32| c + a * hl * end + (u * t.cos() + v * t.sin()) * r;
                                gizmos.line(w(p(a0)), w(p(a1)), col);
                            }
                        }
                        for k in 0..4 {
                            let t = std::f32::consts::FRAC_PI_2 * k as f32;
                            let off = (u * t.cos() + v * t.sin()) * r;
                            gizmos.line(w(c - a * hl + off), w(c + a * hl + off), col);
                        }
                    }
                    CollisionPrim::Sphere { center, radius } => {
                        let c = Vec3::new(center[0] as f32, center[1] as f32, center[2] as f32);
                        let r = *radius as f32;
                        for (u, v) in [(Vec3::X, Vec3::Y), (Vec3::X, Vec3::Z), (Vec3::Y, Vec3::Z)] {
                            for k in 0..32 {
                                let (a0, a1) = (std::f32::consts::TAU * k as f32 / 32.0, std::f32::consts::TAU * (k + 1) as f32 / 32.0);
                                let p = |t: f32| c + (u * t.cos() + v * t.sin()) * r;
                                gizmos.line(w(p(a0)), w(p(a1)), col);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Adds the browser view and overlays.
pub struct BrowserPlugin(pub BrowserState);

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .init_resource::<BrowserView>()
            .add_systems(Update, (rebuild, overlays));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_wrap_and_centre() {
        let sizes = vec![(0.3, 0.1), (0.3, 0.2), (0.3, 0.1)];
        let (c, total) = layout_rows(&sizes, &[false; 3], 0.7, 0.01);
        // First two share a row, the third wraps.
        assert!((c[0].y - c[1].y).abs() < 1e-6 && c[2].y < c[0].y);
        assert!((total.x - 0.61).abs() < 1e-5);
        assert!((total.y - 0.31).abs() < 1e-5);
        // Layout is centred on the origin.
        let (min_x, max_x) = (c[0].x - 0.15, c[1].x + 0.15);
        assert!((min_x + max_x).abs() < 1e-5);
    }

    #[test]
    fn category_change_starts_a_new_row() {
        let sizes = vec![(0.05, 0.05), (0.05, 0.05)];
        let (c, _) = layout_rows(&sizes, &[false, true], 1.0, 0.01);
        assert!(c[1].y < c[0].y);
    }

    #[test]
    fn filter_matches_id_name_and_category() {
        let cat = axl_parts::Catalog::builtin();
        let brick = cat.get("brick_2x4").unwrap();
        assert!(matches_query(brick, ""));
        assert!(matches_query(brick, "ALL"));
        assert!(matches_query(brick, "2x4"));
        assert!(matches_query(brick, "brick"));
        assert!(!matches_query(brick, "gear"));
        assert!(matches_query(cat.get("gear_24").unwrap(), "gear"));
    }

    #[test]
    fn framing_grows_with_the_layout() {
        let small = frame_layout(0.03, 0.02, 0.01);
        let big = frame_layout(0.6, 0.5, 0.05);
        assert!(big.distance > small.distance * 5.0);
    }

    #[test]
    fn stats_report_mass_in_grams() {
        // 2x4 brick: about 2.15 g.
        let cat = axl_parts::Catalog::builtin();
        let d = cat.get("brick_2x4").unwrap();
        let g = d.bodies[0].mass_kg * 1e3;
        assert!((g - 2.15).abs() < 0.05, "{g}");
    }
}
