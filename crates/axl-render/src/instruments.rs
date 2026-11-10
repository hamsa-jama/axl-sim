//! The teaching overlays (SPEC "Teaching overlays and instruments"): frames, balance, contacts,
//! sensors, trails, drawings and X-ray, all drawn from the newest snapshot with gizmos. Nothing
//! here changes the simulation.
//!
//! The app copies each snapshot into [`LiveView`] and the controller's pose and line events into
//! [`Trails`]; this module only draws. Positions arrive in the AXL frame and are converted with
//! [`axl_to_bevy`], the one conversion function.

use std::collections::VecDeque;

use bevy::gizmos::config::{GizmoConfigGroup, GizmoConfigStore};
use bevy::prelude::*;

use crate::camera_rig::{ActiveQuality, UiCapture};
use crate::frames::axl_to_bevy;
use crate::quality::QualityPreset;
use crate::scene::PartMaterials;

/// Seconds of path the trails keep (SPEC: last 60 s).
pub const TRAIL_SECONDS: f64 = 60.0;
/// Length of the frame axes (SPEC: 30 mm).
pub const AXIS_LEN_M: f32 = 0.030;
/// Length of a contact arrow per newton below [`ARROW_LINEAR_LIMIT_N`] (SPEC: 10 mm per newton).
pub const ARROW_M_PER_N: f64 = 0.010;
/// Above this force the arrow length grows logarithmically (SPEC: 5 N).
pub const ARROW_LINEAR_LIMIT_N: f64 = 5.0;
/// Opacity of the parts with X-ray on (SPEC: 30%).
pub const XRAY_ALPHA: f32 = 0.3;

/// Gizmos drawn on top of the robot (depth bias -1), solid.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OverlayGizmos;

/// Gizmos drawn on top of the robot, dashed (the believed path).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DashedGizmos;

/// Which overlays are on. Keys `1` to `7` toggle them in order.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overlays {
    /// Key 1: 30 mm RGB axes on every body.
    pub frames: bool,
    /// Key 2: centre of mass and support polygon.
    pub balance: bool,
    /// Key 3: contacts and force arrows.
    pub contacts: bool,
    /// Key 4: sensor cones, rays, spot and labels.
    pub sensors: bool,
    /// Key 5: true and believed paths.
    pub trails: bool,
    /// Key 6: lines from `axl_draw_line`.
    pub drawings: bool,
    /// Key 7: parts at 30% opacity.
    pub xray: bool,
}

impl Overlays {
    /// Names in key order.
    pub const NAMES: [&'static str; 7] = [
        "Frames", "Balance", "Contacts", "Sensors", "Trails", "Drawings", "X-ray",
    ];

    /// The flag for overlay `i` (0 to 6).
    pub fn get(&self, i: usize) -> bool {
        match i {
            0 => self.frames,
            1 => self.balance,
            2 => self.contacts,
            3 => self.sensors,
            4 => self.trails,
            5 => self.drawings,
            6 => self.xray,
            _ => false,
        }
    }

    /// Set overlay `i` (0 to 6).
    pub fn set(&mut self, i: usize, on: bool) {
        match i {
            0 => self.frames = on,
            1 => self.balance = on,
            2 => self.contacts = on,
            3 => self.sensors = on,
            4 => self.trails = on,
            5 => self.drawings = on,
            6 => self.xray = on,
            _ => {}
        }
    }

    /// Flip overlay `i`.
    pub fn toggle(&mut self, i: usize) {
        let now = self.get(i);
        self.set(i, !now);
    }
}

/// A body as the inspector and the frames overlay need it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveBody {
    /// Position, AXL frame, metres.
    pub pos: [f64; 3],
    /// Orientation `[w, x, y, z]`.
    pub quat: [f64; 4],
    /// Linear velocity of the centre of mass, m/s.
    pub velocity: [f64; 3],
    /// Angular velocity, rad/s.
    pub angular_velocity: [f64; 3],
    /// Centre of mass, metres.
    pub com: [f64; 3],
    /// Mass, kg.
    pub mass: f64,
    /// Principal moments of inertia, kg m^2.
    pub inertia: [f64; 3],
}

/// One contact.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveContact {
    /// Position.
    pub pos: [f64; 3],
    /// Unit normal.
    pub normal: [f64; 3],
    /// Normal force, newtons.
    pub force: f64,
    /// Friction coefficient (for the cone).
    pub mu: f64,
    /// Part of the support polygon.
    pub floor: bool,
}

/// One hinge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveJoint {
    /// Anchor.
    pub anchor: [f64; 3],
    /// Unit axis.
    pub axis: [f64; 3],
    /// Angle, radians.
    pub angle: f64,
    /// Speed, rad/s.
    pub speed: f64,
    /// Torque, N m.
    pub torque: f64,
}

/// Which kind of sensor a [`LiveSensor`] is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SensorKind {
    /// Ultrasonic cone.
    #[default]
    Ultrasonic,
    /// Time-of-flight rays.
    Tof,
    /// Colour spot.
    Color,
}

/// One sensor as the overlay draws it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveSensor {
    /// Device name.
    pub name: String,
    /// Kind.
    pub kind: SensorKind,
    /// Position.
    pub origin: [f64; 3],
    /// Unit view direction.
    pub axis: [f64; 3],
    /// Longest range, metres.
    pub range: f64,
    /// Half angle of the widest ray, degrees.
    pub half_angle_deg: f64,
    /// Ray end points.
    pub ends: Vec<[f64; 3]>,
    /// Whether each ray counted.
    pub hit: Vec<bool>,
    /// The label text: the reading with its unit.
    pub label: String,
    /// Colour sensor reading, 0 to 1023 per channel.
    pub rgb: [i32; 3],
}

/// The newest snapshot, in the plain types the overlays draw. Filled by the app.
#[derive(Resource, Clone, Debug, Default)]
pub struct LiveView {
    /// Simulated seconds.
    pub time: f64,
    /// Every body (index = MuJoCo body id).
    pub bodies: Vec<LiveBody>,
    /// Contacts, already held over micro-bounces (see the app).
    pub contacts: Vec<LiveContact>,
    /// Hinges.
    pub joints: Vec<LiveJoint>,
    /// Robot centre of mass.
    pub com: [f64; 3],
    /// Robot mass.
    pub robot_mass: f64,
    /// The robot's root body.
    pub root: usize,
    /// Distance and colour sensors.
    pub sensors: Vec<LiveSensor>,
    /// The body selected in the inspector.
    pub selected: Option<usize>,
    /// The hinge under the cursor (index into `joints`).
    pub hover_joint: Option<usize>,
    /// The mouse spring while dragging: (grab point, cursor target).
    pub drag: Option<([f64; 3], [f64; 3])>,
}

/// The paths and drawings of a run.
#[derive(Resource, Clone, Debug)]
pub struct Trails {
    /// True path: (time, position).
    pub truth: VecDeque<(f64, [f64; 3])>,
    /// Believed path from `axl_mark_pose`: (time, [x, y, yaw]).
    pub believed: VecDeque<(f64, [f64; 3])>,
    /// Lines from `axl_draw_line`: (from, to, 0xRRGGBBAA).
    pub lines: Vec<([f64; 3], [f64; 3], u32)>,
    /// Footprint of the robot (x, y full size, metres) for the ghost outline.
    pub footprint: [f64; 2],
}

impl Default for Trails {
    fn default() -> Self {
        Self {
            truth: VecDeque::new(),
            believed: VecDeque::new(),
            lines: Vec::new(),
            footprint: [0.2, 0.15],
        }
    }
}

impl Trails {
    /// Forget everything (a restart).
    pub fn clear(&mut self) {
        self.truth.clear();
        self.believed.clear();
        self.lines.clear();
    }

    /// Add a point of the true path (skipped when the robot has hardly moved).
    pub fn push_truth(&mut self, time: f64, pos: [f64; 3]) {
        if let Some((_, last)) = self.truth.back() {
            let d2: f64 = (0..3).map(|i| (pos[i] - last[i]).powi(2)).sum();
            if d2 < 1e-6 {
                // less than 1 mm
                return;
            }
        }
        self.truth.push_back((time, pos));
        trim(&mut self.truth, time);
    }

    /// Add a believed pose.
    pub fn push_believed(&mut self, time: f64, x: f64, y: f64, yaw: f64) {
        self.believed.push_back((time, [x, y, yaw]));
        trim(&mut self.believed, time);
    }
}

fn trim<T>(q: &mut VecDeque<(f64, T)>, now: f64) {
    while q.front().is_some_and(|p| p.0 < now - TRAIL_SECONDS) {
        q.pop_front();
    }
}

// ----- pure helpers ------------------------------------------------------------------------

/// Length in metres of the arrow for a normal force: 10 mm per newton up to 5 N, then
/// logarithmic (50 mm plus 50 mm per e-fold of the force over 5 N).
pub fn arrow_length_m(force_n: f64) -> f64 {
    let f = force_n.max(0.0);
    if f <= ARROW_LINEAR_LIMIT_N {
        f * ARROW_M_PER_N
    } else {
        ARROW_LINEAR_LIMIT_N * ARROW_M_PER_N * (1.0 + (f / ARROW_LINEAR_LIMIT_N).ln())
    }
}

/// The convex hull of 2D points (Andrew's monotone chain), counter-clockwise, without repeating
/// the first point. Fewer than three distinct points give those points back.
pub fn convex_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p: Vec<[f64; 2]> = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut h: Vec<[f64; 2]> = Vec::with_capacity(p.len() * 2);
    for &pt in &p {
        while h.len() >= 2 && cross(h[h.len() - 2], h[h.len() - 1], pt) <= 0.0 {
            h.pop();
        }
        h.push(pt);
    }
    let lower = h.len() + 1;
    for &pt in p.iter().rev().skip(1) {
        while h.len() >= lower && cross(h[h.len() - 2], h[h.len() - 1], pt) <= 0.0 {
            h.pop();
        }
        h.push(pt);
    }
    h.pop();
    h
}

/// Is `p` inside (or on the edge of) the counter-clockwise convex polygon `hull`? A polygon with
/// fewer than three corners holds nothing: a robot resting on two points is tipping.
pub fn point_in_hull(hull: &[[f64; 2]], p: [f64; 2]) -> bool {
    if hull.len() < 3 {
        return false;
    }
    (0..hull.len()).all(|i| {
        let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= -1e-12
    })
}

/// Rotate a vector by a `[w, x, y, z]` quaternion.
pub fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

/// `0xRRGGBBAA` as a colour.
pub fn rgba_color(rgba: u32) -> Color {
    Color::srgba_u8(
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        rgba as u8,
    )
}

fn v3(p: [f64; 3]) -> Vec3 {
    axl_to_bevy(Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32))
}

fn normalised(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

/// Two unit vectors perpendicular to `n` and to each other.
fn tangents(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let n = normalised(n);
    let a = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let t1 = normalised([
        n[1] * a[2] - n[2] * a[1],
        n[2] * a[0] - n[0] * a[2],
        n[0] * a[1] - n[1] * a[0],
    ]);
    let t2 = [
        n[1] * t1[2] - n[2] * t1[1],
        n[2] * t1[0] - n[0] * t1[2],
        n[0] * t1[1] - n[1] * t1[0],
    ];
    (t1, t2)
}

/// The support polygon of a live view: the hull of the floor contacts' x, y.
pub fn support_polygon(contacts: &[LiveContact]) -> Vec<[f64; 2]> {
    let pts: Vec<[f64; 2]> = contacts
        .iter()
        .filter(|c| c.floor && c.force > 0.0)
        .map(|c| [c.pos[0], c.pos[1]])
        .collect();
    convex_hull(&pts)
}

// ----- drawing -----------------------------------------------------------------------------

const RED: Color = Color::srgb(0.90, 0.22, 0.21);
const GREEN: Color = Color::srgb(0.26, 0.63, 0.28);
const BLUE: Color = Color::srgb(0.12, 0.53, 0.90);
const ORANGE: Color = Color::srgb(1.0, 0.54, 0.09);
const WHITE: Color = Color::srgb(0.95, 0.95, 0.95);

fn draw_circle(
    g: &mut Gizmos<OverlayGizmos>,
    centre: [f64; 3],
    axis: [f64; 3],
    r: f64,
    col: Color,
) {
    let (t1, t2) = tangents(axis);
    let n = 24;
    let pts = (0..=n).map(|i| {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        v3(std::array::from_fn(|k| {
            centre[k] + r * (a.cos() * t1[k] + a.sin() * t2[k])
        }))
    });
    g.linestrip(pts, col);
}

fn draw_frames(g: &mut Gizmos<OverlayGizmos>, view: &LiveView) {
    for b in view.bodies.iter().skip(1) {
        for (i, col) in [RED, GREEN, BLUE].into_iter().enumerate() {
            let mut axis = [0.0; 3];
            axis[i] = f64::from(AXIS_LEN_M);
            let tip = rotate(b.quat, axis);
            g.line(
                v3(b.pos),
                v3([b.pos[0] + tip[0], b.pos[1] + tip[1], b.pos[2] + tip[2]]),
                col,
            );
        }
    }
    // The world frame at the origin.
    for (i, col) in [RED, GREEN, BLUE].into_iter().enumerate() {
        let mut tip = [0.0; 3];
        tip[i] = f64::from(AXIS_LEN_M);
        g.line(v3([0.0; 3]), v3(tip), col);
    }
}

fn draw_balance(g: &mut Gizmos<OverlayGizmos>, view: &LiveView) {
    let com = view.com;
    let hull = support_polygon(&view.contacts);
    let inside = point_in_hull(&hull, [com[0], com[1]]);
    let col = if inside { GREEN } else { RED };
    g.sphere(Isometry3d::from_translation(v3(com)), 0.006, ORANGE);
    let foot = [com[0], com[1], 0.0];
    g.line(v3(com), v3(foot), col);
    if hull.len() >= 2 {
        let lift = |p: &[f64; 2]| v3([p[0], p[1], 0.001]);
        let mut ring: Vec<Vec3> = hull.iter().map(lift).collect();
        if hull.len() >= 3 {
            ring.push(ring[0]);
        }
        g.linestrip(ring, WHITE);
    }
    // A tick when the dropped point is inside the polygon, a cross when it is not, so the colour
    // is never the only signal.
    let s = 0.012;
    let at = |dx: f64, dy: f64| v3([foot[0] + dx, foot[1] + dy, 0.002]);
    if inside {
        g.linestrip([at(-s, 0.0), at(-s * 0.3, -s * 0.8), at(s * 1.2, s)], col);
    } else {
        g.line(at(-s, -s), at(s, s), col);
        g.line(at(-s, s), at(s, -s), col);
    }
}

fn draw_contacts(g: &mut Gizmos<OverlayGizmos>, view: &LiveView, cones: bool) {
    for c in &view.contacts {
        let p = v3(c.pos);
        g.sphere(Isometry3d::from_translation(p), 0.0015, WHITE);
        let n = normalised(c.normal);
        let len = arrow_length_m(c.force);
        if len > 1e-4 {
            let tip = [
                c.pos[0] + n[0] * len,
                c.pos[1] + n[1] * len,
                c.pos[2] + n[2] * len,
            ];
            g.arrow(p, v3(tip), ORANGE);
        }
        if cones && c.force > 0.05 && c.mu > 0.0 {
            // The friction cone: friction stays inside it. Apex at the contact, axis along the
            // normal, half angle atan(mu), as tall as the force arrow.
            let h = len.max(0.01);
            let r = h * c.mu;
            let axis_tip = [
                c.pos[0] + n[0] * h,
                c.pos[1] + n[1] * h,
                c.pos[2] + n[2] * h,
            ];
            draw_circle(g, axis_tip, n, r, Color::srgba(1.0, 0.8, 0.3, 0.8));
            let (t1, t2) = tangents(n);
            for (a, b) in [(t1, 1.0), (t1, -1.0), (t2, 1.0), (t2, -1.0)] {
                let rim = [
                    axis_tip[0] + a[0] * r * b,
                    axis_tip[1] + a[1] * r * b,
                    axis_tip[2] + a[2] * r * b,
                ];
                g.line(p, v3(rim), Color::srgba(1.0, 0.8, 0.3, 0.8));
            }
        }
    }
}

fn draw_sensors(g: &mut Gizmos<OverlayGizmos>, view: &LiveView) {
    for s in &view.sensors {
        let o = s.origin;
        let axis = normalised(s.axis);
        match s.kind {
            SensorKind::Ultrasonic | SensorKind::Tof => {
                // The cone: rim circle at full range and four edge lines.
                let half = s.half_angle_deg.to_radians();
                let r = s.range * half.tan();
                let tip = [
                    o[0] + axis[0] * s.range,
                    o[1] + axis[1] * s.range,
                    o[2] + axis[2] * s.range,
                ];
                let cone = Color::srgba(0.30, 0.75, 1.0, 0.7);
                draw_circle(g, tip, axis, r, cone);
                let (t1, t2) = tangents(axis);
                for (a, b) in [(t1, 1.0), (t1, -1.0), (t2, 1.0), (t2, -1.0)] {
                    let rim = [
                        tip[0] + a[0] * r * b,
                        tip[1] + a[1] * r * b,
                        tip[2] + a[2] * r * b,
                    ];
                    g.line(v3(o), v3(rim), cone);
                }
                for (end, hit) in s.ends.iter().zip(&s.hit) {
                    let col = if *hit {
                        GREEN
                    } else {
                        Color::srgba(0.6, 0.6, 0.6, 0.6)
                    };
                    g.line(v3(o), v3(*end), col);
                    if *hit {
                        g.sphere(Isometry3d::from_translation(v3(*end)), 0.002, GREEN);
                    }
                }
            }
            SensorKind::Color => {
                if let (Some(end), Some(hit)) = (s.ends.first(), s.hit.first()) {
                    g.line(v3(o), v3(*end), Color::srgba(1.0, 1.0, 1.0, 0.6));
                    let col = Color::srgb(
                        (s.rgb[0] as f32 / 1023.0).clamp(0.0, 1.0),
                        (s.rgb[1] as f32 / 1023.0).clamp(0.0, 1.0),
                        (s.rgb[2] as f32 / 1023.0).clamp(0.0, 1.0),
                    );
                    // The 3 mm spot, drawn as two rings so it shows up on any floor.
                    let c = if *hit { *end } else { o };
                    draw_circle(g, c, axis, 0.003, col);
                    draw_circle(g, c, axis, 0.0035, Color::BLACK);
                }
            }
        }
    }
}

fn draw_trails(g: &mut Gizmos<OverlayGizmos>, d: &mut Gizmos<DashedGizmos>, trails: &Trails) {
    let lift = |p: [f64; 3]| v3([p[0], p[1], p[2].max(0.002)]);
    if trails.truth.len() >= 2 {
        g.linestrip(
            trails.truth.iter().map(|(_, p)| lift(*p)),
            Color::srgb(0.2, 0.9, 0.9),
        );
    }
    if trails.believed.len() >= 2 {
        d.linestrip(
            trails.believed.iter().map(|(_, p)| v3([p[0], p[1], 0.003])),
            Color::srgb(1.0, 0.3, 0.8),
        );
    }
    if let Some((_, [x, y, yaw])) = trails.believed.back() {
        // The ghost outline of the robot at its believed pose, with a nose to show the heading.
        let (hx, hy) = (trails.footprint[0] * 0.5, trails.footprint[1] * 0.5);
        let (c, s) = (yaw.cos(), yaw.sin());
        let corner = |lx: f64, ly: f64| v3([x + c * lx - s * ly, y + s * lx + c * ly, 0.003]);
        let col = Color::srgb(1.0, 0.3, 0.8);
        let ring = [
            corner(hx, hy),
            corner(hx, -hy),
            corner(-hx, -hy),
            corner(-hx, hy),
            corner(hx, hy),
        ];
        g.linestrip(ring, col);
        g.line(corner(0.0, 0.0), corner(hx * 1.3, 0.0), col);
    }
}

fn draw_xray_joints(g: &mut Gizmos<OverlayGizmos>, view: &LiveView) {
    for j in &view.joints {
        draw_circle(g, j.anchor, j.axis, 0.008, Color::srgba(1.0, 0.8, 0.2, 0.9));
        let a = normalised(j.axis);
        let (p, q) = (
            [
                j.anchor[0] - a[0] * 0.01,
                j.anchor[1] - a[1] * 0.01,
                j.anchor[2] - a[2] * 0.01,
            ],
            [
                j.anchor[0] + a[0] * 0.01,
                j.anchor[1] + a[1] * 0.01,
                j.anchor[2] + a[2] * 0.01,
            ],
        );
        g.line(v3(p), v3(q), Color::srgba(1.0, 0.8, 0.2, 0.9));
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_overlays(
    overlays: Res<Overlays>,
    view: Option<Res<LiveView>>,
    trails: Option<Res<Trails>>,
    quality: Option<Res<ActiveQuality>>,
    mut g: Gizmos<OverlayGizmos>,
    mut dashed: Gizmos<DashedGizmos>,
) {
    let Some(view) = view else { return };
    if overlays.frames {
        draw_frames(&mut g, &view);
    }
    if overlays.balance {
        draw_balance(&mut g, &view);
    }
    if overlays.contacts {
        let cones =
            quality.is_some_and(|q| matches!(q.0, QualityPreset::High | QualityPreset::Ultra));
        draw_contacts(&mut g, &view, cones);
    }
    if overlays.sensors {
        draw_sensors(&mut g, &view);
    }
    if let Some(trails) = trails {
        if overlays.trails {
            draw_trails(&mut g, &mut dashed, &trails);
        }
        if overlays.drawings {
            for (a, b, rgba) in &trails.lines {
                g.line(v3(*a), v3(*b), rgba_color(*rgba));
            }
        }
    }
    if overlays.xray {
        draw_xray_joints(&mut g, &view);
    }
    if let Some((a, b)) = view.drag {
        g.line(v3(a), v3(b), Color::srgb(1.0, 0.9, 0.2));
        g.sphere(
            Isometry3d::from_translation(v3(b)),
            0.004,
            Color::srgb(1.0, 0.9, 0.2),
        );
    }
    // The selected body and the hovered hinge are always marked.
    if let Some(b) = view.selected.and_then(|i| view.bodies.get(i)) {
        g.sphere(Isometry3d::from_translation(v3(b.com)), 0.009, ORANGE);
    }
    if let Some(j) = view.hover_joint.and_then(|i| view.joints.get(i)) {
        draw_circle(&mut g, j.anchor, j.axis, 0.012, ORANGE);
        let a = normalised(j.axis);
        g.line(
            v3([
                j.anchor[0] - a[0] * 0.03,
                j.anchor[1] - a[1] * 0.03,
                j.anchor[2] - a[2] * 0.03,
            ]),
            v3([
                j.anchor[0] + a[0] * 0.03,
                j.anchor[1] + a[1] * 0.03,
                j.anchor[2] + a[2] * 0.03,
            ]),
            ORANGE,
        );
    }
}

/// Keys `1` to `7` toggle the overlays.
fn overlay_keys(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<UiCapture>,
    mut overlays: ResMut<Overlays>,
) {
    if ui.keyboard {
        return;
    }
    let codes = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, code) in codes.into_iter().enumerate() {
        if keys.just_pressed(code) {
            overlays.toggle(i);
        }
    }
}

/// Parts at 30% opacity while X-ray is on: every cached part material goes translucent (and
/// back).
fn apply_xray(
    overlays: Res<Overlays>,
    parts: Option<Res<PartMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<bool>,
) {
    let Some(parts) = parts else { return };
    if overlays.xray == *last && !parts.is_changed() {
        return;
    }
    *last = overlays.xray;
    for h in parts.material_handles() {
        if let Some(mut m) = materials.get_mut(&h) {
            let mut c = m.base_color.to_linear();
            c.alpha = if overlays.xray { XRAY_ALPHA } else { 1.0 };
            m.base_color = c.into();
            m.alpha_mode = if overlays.xray {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            };
        }
    }
}

/// Registers the overlays: the gizmo groups, the flags, the keys and the drawing systems.
pub struct InstrumentsPlugin;

impl Plugin for InstrumentsPlugin {
    fn build(&self, app: &mut App) {
        use bevy::gizmos::AppGizmoBuilder as _;
        use bevy::gizmos::config::{GizmoConfig, GizmoLineConfig, GizmoLineStyle};
        let solid = GizmoConfig {
            line: GizmoLineConfig {
                width: 2.5,
                ..default()
            },
            depth_bias: -1.0,
            ..default()
        };
        let dashed = GizmoConfig {
            line: GizmoLineConfig {
                width: 2.5,
                style: GizmoLineStyle::Dashed {
                    gap_scale: 3.0,
                    line_scale: 6.0,
                },
                ..default()
            },
            depth_bias: -1.0,
            ..default()
        };
        app.init_gizmo_group::<OverlayGizmos>()
            .init_gizmo_group::<DashedGizmos>()
            .insert_resource(Overlays::default())
            .init_resource::<Trails>()
            .init_resource::<LiveView>()
            .add_systems(
                Update,
                (overlay_keys, draw_overlays, apply_xray)
                    .chain()
                    .run_if(crate::stage_swap::in_sim),
            );
        let mut store = app.world_mut().resource_mut::<GizmoConfigStore>();
        *store.config_mut::<OverlayGizmos>().0 = solid;
        *store.config_mut::<DashedGizmos>().0 = dashed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_are_10_mm_per_newton_then_logarithmic() {
        assert_eq!(arrow_length_m(0.0), 0.0);
        assert!((arrow_length_m(1.0) - 0.010).abs() < 1e-12);
        assert!((arrow_length_m(5.0) - 0.050).abs() < 1e-12);
        // Continuous at 5 N, and 50 mm more for each factor e above it.
        assert!((arrow_length_m(5.0 + 1e-9) - 0.050).abs() < 1e-6);
        assert!((arrow_length_m(5.0 * std::f64::consts::E) - 0.100).abs() < 1e-9);
        // Always growing, never past linear.
        assert!(arrow_length_m(50.0) > arrow_length_m(10.0));
        assert!(arrow_length_m(50.0) < 0.5);
        assert_eq!(arrow_length_m(-3.0), 0.0);
    }

    #[test]
    fn hull_of_a_square_with_interior_points() {
        let pts = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.5, 0.5],
            [0.2, 0.7],
            [1.0, 0.5],
        ];
        let h = convex_hull(&pts);
        assert_eq!(h.len(), 4);
        for c in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            assert!(h.contains(&c));
        }
        assert!(point_in_hull(&h, [0.5, 0.5]));
        assert!(point_in_hull(&h, [1.0, 0.5]), "the edge counts as inside");
        assert!(!point_in_hull(&h, [1.2, 0.5]));
        assert!(!point_in_hull(&h, [-0.01, 0.5]));
    }

    #[test]
    fn degenerate_hulls_hold_nothing() {
        assert!(convex_hull(&[]).is_empty());
        assert_eq!(convex_hull(&[[1.0, 1.0], [1.0, 1.0]]), vec![[1.0, 1.0]]);
        let line = convex_hull(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]);
        assert!(!point_in_hull(&line, [1.0, 0.0]));
        assert!(!point_in_hull(&[], [0.0, 0.0]));
    }

    #[test]
    fn support_polygon_uses_only_loaded_floor_contacts() {
        let c = |x: f64, y: f64, f: f64, floor: bool| LiveContact {
            pos: [x, y, 0.0],
            force: f,
            floor,
            ..default()
        };
        let hull = support_polygon(&[
            c(0.0, 0.0, 1.0, true),
            c(1.0, 0.0, 1.0, true),
            c(0.0, 1.0, 1.0, true),
            c(9.0, 9.0, 1.0, false),
            c(5.0, 5.0, 0.0, true),
        ]);
        assert_eq!(hull.len(), 3);
        assert!(point_in_hull(&hull, [0.2, 0.2]));
        assert!(!point_in_hull(&hull, [0.8, 0.8]));
    }

    #[test]
    fn quaternion_rotation_matches_a_quarter_turn() {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let q = [h, 0.0, 0.0, h]; // 90 degrees about z
        let v = rotate(q, [1.0, 0.0, 0.0]);
        assert!((v[0]).abs() < 1e-12 && (v[1] - 1.0).abs() < 1e-12 && v[2].abs() < 1e-12);
        let id = rotate([1.0, 0.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
        assert_eq!(id, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn overlay_flags_toggle_by_index() {
        let mut o = Overlays::default();
        for i in 0..7 {
            assert!(!o.get(i));
            o.toggle(i);
            assert!(o.get(i));
        }
        assert!(
            o.frames && o.balance && o.contacts && o.sensors && o.trails && o.drawings && o.xray
        );
        o.toggle(6);
        assert!(!o.xray);
        o.toggle(9); // ignored
        assert!(!o.get(9));
    }

    #[test]
    fn trails_keep_sixty_seconds_and_skip_standing_still() {
        let mut t = Trails::default();
        t.push_truth(0.0, [0.0, 0.0, 0.0]);
        t.push_truth(0.1, [0.0005, 0.0, 0.0]); // under 1 mm: skipped
        assert_eq!(t.truth.len(), 1);
        t.push_truth(0.2, [0.1, 0.0, 0.0]);
        t.push_truth(70.0, [0.2, 0.0, 0.0]);
        assert_eq!(t.truth.len(), 1, "old points fall off after 60 s");
        t.push_believed(0.0, 0.0, 0.0, 0.0);
        t.push_believed(61.0, 1.0, 0.0, 0.0);
        assert_eq!(t.believed.len(), 1);
        t.clear();
        assert!(t.truth.is_empty() && t.believed.is_empty() && t.lines.is_empty());
    }

    #[test]
    fn rgba_is_red_green_blue_alpha() {
        let c = rgba_color(0xFF8000C0).to_srgba();
        assert!((c.red - 1.0).abs() < 1e-3);
        assert!((c.green - 128.0 / 255.0).abs() < 1e-3);
        assert!(c.blue.abs() < 1e-3);
        assert!((c.alpha - 192.0 / 255.0).abs() < 1e-3);
    }
}
