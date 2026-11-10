//! The instruments around a running simulation (M8): time controls, the overlay switches, the
//! inspector, the joint hover, the plotter and the mouse spring.
//!
//! Used by the sandbox (`--sandbox`) and the Code screen. The simulation lives on its own thread;
//! this file only sends it commands, reads its snapshots and events, and draws egui. The
//! overlays themselves are drawn by `axl_render::instruments`.

use std::collections::{BTreeMap, BTreeSet};

use axl_render::bevy::prelude::*;
use axl_render::bevy::window::PrimaryWindow;
use axl_render::camera_rig::{UiCapture, ViewCamera, ViewInsets};
use axl_render::frames::{axl_to_bevy, bevy_to_axl};
use axl_render::instruments::{
    LiveBody, LiveContact, LiveJoint, LiveSensor, LiveView, Overlays, SensorKind, Trails,
};
use axl_render::sim_view::SandboxScene;
use axl_render::sync::{BodyPose, RenderSnapshot, SnapshotBuffer};
use axl_sim::plot::PlotStore;
use axl_sim::{Command, SimEvent, SimSnapshot};
use bevy_egui::{EguiContexts, egui};

use crate::browser_ui::TopInset;
use crate::sandbox::SandboxSim;

/// Speeds the `[` and `]` keys step through.
pub const SPEEDS: [f64; 6] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0];
/// Seconds a floor contact is kept after it vanishes, so the support polygon does not flicker
/// while a resting robot micro-bounces.
const CONTACT_HOLD_S: f64 = 0.07;
/// Plot and trail samples per simulated second.
const SAMPLE_HZ: f64 = 50.0;
/// Time windows the plotter offers, seconds.
const WINDOWS: [f64; 5] = [5.0, 10.0, 20.0, 30.0, 60.0];
/// How stiff the mouse spring is, as a rate: the dragged body is pulled with `mass * OMEGA^2` per
/// metre. A feel parameter of the interface, not a physical constant.
const SPRING_OMEGA: f64 = 9.0;
/// Damping ratio of the mouse spring.
const SPRING_ZETA: f64 = 0.7;
/// The spring never pulls with more than this many times the robot's weight.
const SPRING_MAX_WEIGHTS: f64 = 1.5;

/// A drag in progress.
#[derive(Clone, Copy, Debug)]
struct Drag {
    body: usize,
    /// Grab point in the body's frame.
    local: [f64; 3],
    /// Fixed target (the scripted demo), instead of following the cursor.
    fixed_target: Option<[f64; 3]>,
}

/// Everything the instruments remember between frames.
#[derive(Resource)]
pub struct Instruments {
    /// Plotter data.
    pub plots: PlotStore,
    /// Events the instruments do not use (log lines, started, ended), for the Code screen.
    pub inbox: Vec<SimEvent>,
    /// Keep `inbox` (the Code screen reads it).
    pub collect_inbox: bool,
    /// The plotter window is open.
    pub plot_open: bool,
    /// Test hook: write the CSV here instead of asking where.
    pub csv_path: Option<std::path::PathBuf>,
    /// The last export, for the plotter's status line.
    pub csv_note: String,
    /// Scripted actions for screenshots.
    pub script: Script,
    max_time: f64,
    checkpoints: BTreeMap<u64, Vec<BodyPose>>,
    scrub: Option<f64>,
    channels_on: BTreeSet<String>,
    window_s: f64,
    frozen: Option<f64>,
    last_sample: f64,
    held: Vec<LiveContact>,
    held_at: f64,
    drag: Option<Drag>,
    mouse_held: bool,
    pick_seq: u64,
    pending_pick: Option<u64>,
    cursor: Option<Vec2>,
    // Mirrors of the newest snapshot, for the egui side.
    time: f64,
    paused: bool,
    speed: f64,
    plot_cursor: Option<f64>,
    replay: Option<f64>,
    preview_key: Option<u64>,
    last_pushed: f64,
}

/// Actions a screenshot run asks for on the command line.
#[derive(Clone, Debug, Default)]
pub struct Script {
    /// Open the plotter.
    pub plots: bool,
    /// Select this body id once the snapshot exists.
    pub select: Option<usize>,
    /// Pause at this simulated time.
    pub pause_at: Option<f64>,
    /// Write the CSV (to `csv_path`) at this simulated time.
    pub export_at: Option<f64>,
    /// Start a scripted drag of the robot at this simulated time.
    pub drag_at: Option<f64>,
    /// Rewind to this time at `rewind_at`.
    pub rewind: Option<(f64, f64)>,
    /// Hover the first hinge (its tooltip shows).
    pub hover_joint: bool,
}

impl Default for Instruments {
    fn default() -> Self {
        Self {
            plots: PlotStore::new(),
            inbox: Vec::new(),
            collect_inbox: false,
            plot_open: false,
            csv_path: std::env::var_os("AXL_CSV_EXPORT_PATH").map(Into::into),
            csv_note: String::new(),
            script: Script::default(),
            max_time: 0.0,
            checkpoints: BTreeMap::new(),
            scrub: None,
            channels_on: BTreeSet::new(),
            window_s: 10.0,
            frozen: None,
            last_sample: f64::NEG_INFINITY,
            held: Vec::new(),
            held_at: f64::NEG_INFINITY,
            drag: None,
            mouse_held: false,
            pick_seq: 0,
            pending_pick: None,
            cursor: None,
            time: 0.0,
            paused: false,
            speed: 1.0,
            plot_cursor: None,
            replay: None,
            preview_key: None,
            last_pushed: f64::NEG_INFINITY,
        }
    }
}

impl Instruments {
    /// Forget the run (a new program started): data, checkpoints, drag.
    pub fn new_run(&mut self) {
        self.plots = PlotStore::new();
        self.channels_on.clear();
        self.checkpoints.clear();
        self.max_time = 0.0;
        self.scrub = None;
        self.frozen = None;
        self.last_sample = f64::NEG_INFINITY;
        self.drag = None;
        self.pending_pick = None;
        self.preview_key = None;
        self.last_pushed = f64::NEG_INFINITY;
    }
}

fn send(sim: &SandboxSim, c: Command) {
    if let Ok(sb) = sim.0.lock() {
        sb.send(c);
    }
}

/// The step from `speed` one notch up or down the [`SPEEDS`] list.
pub fn next_speed(speed: f64, up: bool) -> f64 {
    let i = SPEEDS
        .iter()
        .position(|s| (s - speed).abs() < 1e-9)
        .unwrap_or_else(|| {
            SPEEDS
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1 - speed).abs().total_cmp(&(b.1 - speed).abs()))
                .map_or(3, |(i, _)| i)
        });
    let j = if up {
        (i + 1).min(SPEEDS.len() - 1)
    } else {
        i.saturating_sub(1)
    };
    SPEEDS[j]
}

fn body_pose(p: &axl_mujoco::Pose) -> BodyPose {
    BodyPose {
        pos: p.pos,
        quat: p.quat,
    }
}

fn live_sensor(s: &SimSnapshot, v: &axl_hw::SensorVisual) -> LiveSensor {
    use axl_hw::VisualKind;
    let n = v.n_rays.min(v.ends.len());
    let name = s.device_names.get(v.device).cloned().unwrap_or_default();
    let (kind, label) = match v.kind {
        VisualKind::Ultrasonic | VisualKind::Tof => {
            let label = if v.reading < 0.0 {
                format!("{name}: no echo")
            } else {
                format!("{name}: {:.1} cm", v.reading * 100.0)
            };
            (
                if v.kind == VisualKind::Ultrasonic {
                    SensorKind::Ultrasonic
                } else {
                    SensorKind::Tof
                },
                label,
            )
        }
        VisualKind::Color => (
            SensorKind::Color,
            format!("{name}: {:?}, reflect {:.0}%", v.color_name, v.reading),
        ),
    };
    LiveSensor {
        name,
        kind,
        origin: v.origin,
        axis: v.axis,
        range: v.range_m,
        half_angle_deg: v.half_angle_deg,
        ends: v.ends[..n].to_vec(),
        hit: v.hit[..n].to_vec(),
        label,
        rgb: v.rgb,
    }
}

/// Copy the snapshot into the plain types the overlays draw.
fn fill_live(live: &mut LiveView, s: &SimSnapshot, contacts: &[LiveContact]) {
    live.time = s.time;
    live.bodies = s
        .bodies
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let info = s.body_info.get(i).copied().unwrap_or_default();
            LiveBody {
                pos: p.pos,
                quat: p.quat,
                velocity: info.velocity,
                angular_velocity: info.angular_velocity,
                com: info.com,
                mass: info.mass,
                inertia: info.inertia,
            }
        })
        .collect();
    live.contacts = contacts.to_vec();
    live.joints = s
        .joints
        .iter()
        .map(|j| LiveJoint {
            anchor: j.anchor,
            axis: j.axis,
            angle: j.angle,
            speed: j.speed,
            torque: j.torque,
        })
        .collect();
    live.com = s.com;
    live.robot_mass = s.robot_mass;
    live.root = s.root_body;
    live.sensors = s.sensors.iter().map(|v| live_sensor(s, v)).collect();
}

/// Read events and the newest snapshot from the sim thread; feed the plots, the trails, the
/// overlays' view and the renderer's snapshot buffer.
#[allow(clippy::too_many_arguments)]
pub fn instruments_tick(
    sim: Res<SandboxSim>,
    time: Res<Time<Real>>,
    mut ins: ResMut<Instruments>,
    mut live: ResMut<LiveView>,
    mut trails: ResMut<Trails>,
    mut buf: ResMut<SnapshotBuffer>,
) {
    let Ok(mut sb) = sim.0.lock() else { return };
    let ins = &mut *ins;
    let events: Vec<SimEvent> = sb.events().try_iter().collect();
    let mut picked = None;
    for ev in events {
        match ev {
            SimEvent::Plot {
                channel,
                time,
                value,
            } => {
                if ins.plots.get(&channel).is_none() {
                    ins.channels_on.insert(channel.clone());
                }
                ins.plots.push(&channel, false, time, value);
            }
            SimEvent::Pose { time, x, y, yaw } => trails.push_believed(time, x, y, yaw),
            SimEvent::Line { from, to, rgba } => trails.lines.push((from, to, rgba)),
            SimEvent::ClearLines => trails.lines.clear(),
            SimEvent::Restarted { .. } => {
                ins.plots.clear_points();
                trails.clear();
                ins.last_sample = f64::NEG_INFINITY;
                ins.last_pushed = f64::NEG_INFINITY;
                if ins.collect_inbox {
                    ins.inbox.push(ev);
                }
            }
            SimEvent::Checkpoint { time, bodies, .. } => {
                ins.checkpoints
                    .insert(time as u64, bodies.iter().map(body_pose).collect());
            }
            SimEvent::Picked { id, hit } => {
                if ins.pending_pick == Some(id) {
                    picked = Some(hit);
                }
            }
            other => {
                if ins.collect_inbox {
                    ins.inbox.push(other);
                }
            }
        }
    }
    let s = sb.latest();
    if s.bodies.is_empty() {
        return;
    }
    ins.time = s.time;
    ins.paused = s.paused;
    ins.speed = s.speed;
    ins.replay = s.replay_target;
    ins.max_time = ins.max_time.max(s.time);

    // Floor contacts are kept for a moment after they vanish.
    let now = time.elapsed_secs_f64();
    let fresh: Vec<LiveContact> = s
        .contacts
        .iter()
        .map(|c| LiveContact {
            pos: c.pos,
            normal: c.normal,
            force: c.normal_force,
            mu: c.mu,
            floor: c.floor,
        })
        .collect();
    if fresh.iter().any(|c| c.floor) {
        ins.held.clone_from(&fresh);
        ins.held_at = now;
    }
    let contacts = if fresh.iter().any(|c| c.floor) || now - ins.held_at > CONTACT_HOLD_S {
        fresh
    } else {
        ins.held.clone()
    };
    fill_live(&mut live, s, &contacts);

    // A restart not announced by an event (a time that went backwards).
    if s.time + 1e-9 < ins.last_sample {
        ins.last_sample = f64::NEG_INFINITY;
    }
    if s.time - ins.last_sample >= 1.0 / SAMPLE_HZ {
        ins.last_sample = s.time;
        for (name, value) in s.channel_names.iter().zip(&s.channel_values) {
            if ins.plots.get(name).is_none() && ins.channels_on.len() < 2 {
                ins.channels_on.insert(name.clone());
            }
            ins.plots.push(name, true, s.time, *value);
        }
        if let Some(root) = s.bodies.get(s.root_body) {
            trails.push_truth(s.time, root.pos);
        }
    }

    // The renderer: the live poses, or the checkpoint under the scrub bar.
    if let Some(t) = ins.scrub {
        let key = t.floor().max(0.0) as u64;
        let cp = ins
            .checkpoints
            .range(..=key)
            .next_back()
            .map(|(k, v)| (*k, v.clone()));
        if let Some((k, poses)) = cp
            && ins.preview_key != Some(k)
        {
            ins.preview_key = Some(k);
            buf.push(
                RenderSnapshot {
                    time: k as f64,
                    bodies: poses,
                },
                time.elapsed_secs_f64(),
            );
        }
    } else if s.time != ins.last_pushed {
        ins.last_pushed = s.time;
        ins.preview_key = None;
        buf.push(
            RenderSnapshot {
                time: s.time,
                bodies: s.bodies.iter().map(body_pose).collect(),
            },
            time.elapsed_secs_f64(),
        );
    }

    // The pick answer: select, and start a drag if the button is still down.
    if let Some(hit) = picked {
        ins.pending_pick = None;
        match hit {
            Some(h) => {
                live.selected = Some(h.body);
                if ins.mouse_held
                    && let Some(b) = live.bodies.get(h.body)
                {
                    let r = axl_render::instruments::rotate(
                        [b.quat[0], -b.quat[1], -b.quat[2], -b.quat[3]],
                        [
                            h.point[0] - b.pos[0],
                            h.point[1] - b.pos[1],
                            h.point[2] - b.pos[2],
                        ],
                    );
                    ins.drag = Some(Drag {
                        body: h.body,
                        local: r,
                        fixed_target: None,
                    });
                }
            }
            None => live.selected = None,
        }
    }
    drop(sb);
}

/// Where the cursor ray starts and points, in the AXL frame. The camera projection is shifted to
/// the middle of the free area (see `ViewInsets`), which the main-world ray maths does not know.
fn cursor_ray(
    window: &Window,
    cam: &Camera,
    gt: &GlobalTransform,
    insets: Option<&ViewInsets>,
) -> Option<(Vec3, Vec3)> {
    let pos = window.cursor_position()?;
    let ray = cam
        .viewport_to_world(gt, pos + inset_shift(window, insets))
        .ok()?;
    Some((bevy_to_axl(ray.origin), bevy_to_axl(*ray.direction)))
}

/// Pixels (logical) to add to a cursor position to get the unshifted projection's position.
fn inset_shift(window: &Window, insets: Option<&ViewInsets>) -> Vec2 {
    let Some([l, t, r, b]) = insets.and_then(|i| i.rect) else {
        return Vec2::ZERO;
    };
    let win_c = Vec2::new(window.width(), window.height()) * 0.5;
    let free_c = Vec2::new((l + r) * 0.5, (t + b) * 0.5);
    win_c - free_c
}

fn to_screen(
    cam: &Camera,
    gt: &GlobalTransform,
    window: &Window,
    insets: Option<&ViewInsets>,
    p: [f64; 3],
) -> Option<Vec2> {
    let v = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32);
    cam.world_to_viewport(gt, axl_to_bevy(v))
        .ok()
        .map(|s| s - inset_shift(window, insets))
}

fn v3(v: Vec3) -> [f64; 3] {
    [f64::from(v.x), f64::from(v.y), f64::from(v.z)]
}

/// Keys and the mouse: time controls, pick, the mouse spring and the joint hover.
#[allow(clippy::too_many_arguments)]
pub fn instruments_input(
    sim: Res<SandboxSim>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    ui: Res<UiCapture>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<ViewCamera>>,
    insets: Option<Res<ViewInsets>>,
    time: Res<Time<Real>>,
    mut ins: ResMut<Instruments>,
    mut live: ResMut<LiveView>,
) {
    let ins = &mut *ins;
    // ---- time controls ----
    if !ui.keyboard {
        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        if keys.just_pressed(KeyCode::Space) {
            send(
                &sim,
                if ins.paused {
                    Command::Resume
                } else {
                    Command::Pause
                },
            );
        }
        if keys.just_pressed(KeyCode::Period) {
            if !ins.paused {
                send(&sim, Command::Pause);
            }
            send(
                &sim,
                Command::Step {
                    ms: if shift { 10 } else { 1 },
                },
            );
        }
        if keys.just_pressed(KeyCode::BracketLeft) {
            send(&sim, Command::SetSpeed(next_speed(ins.speed, false)));
        }
        if keys.just_pressed(KeyCode::BracketRight) {
            send(&sim, Command::SetSpeed(next_speed(ins.speed, true)));
        }
        if ctrl && keys.just_pressed(KeyCode::KeyR) {
            send(&sim, Command::Reset);
        }
    }

    // ---- scripted actions (screenshots) ----
    let t = ins.time;
    if let Some(at) = ins.script.pause_at
        && t >= at
    {
        ins.script.pause_at = None;
        send(&sim, Command::Pause);
    }
    if let Some((at, to)) = ins.script.rewind
        && t >= at
    {
        ins.script.rewind = None;
        send(&sim, Command::Rewind(to));
    }
    if let Some(i) = ins.script.select
        && live.bodies.len() > i
    {
        live.selected = Some(i);
        ins.script.select = None;
    }
    if let Some(at) = ins.script.drag_at
        && t >= at
        && !live.bodies.is_empty()
    {
        ins.script.drag_at = None;
        let b = live.bodies[live.root.min(live.bodies.len() - 1)];
        let target = [b.pos[0] + 0.12, b.pos[1] + 0.08, b.pos[2] + 0.06];
        ins.drag = Some(Drag {
            body: live.root,
            local: [0.0, 0.0, 0.02],
            fixed_target: Some(target),
        });
    }
    if ins.script.hover_joint && !live.joints.is_empty() {
        live.hover_joint = Some(0);
    }

    let (Ok(window), Ok((cam, gt))) = (windows.single(), cams.single()) else {
        return;
    };
    let insets = insets.as_deref();
    ins.cursor = window.cursor_position();

    // ---- mouse: pick on press (no Alt, which orbits), spring while held ----
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    if mouse.just_pressed(MouseButton::Left)
        && !ui.pointer
        && !alt
        && let Some((o, d)) = cursor_ray(window, cam, gt, insets)
    {
        ins.mouse_held = true;
        ins.pick_seq += 1;
        ins.pending_pick = Some(ins.pick_seq);
        send(
            &sim,
            Command::Pick {
                id: ins.pick_seq,
                origin: v3(o),
                dir: v3(d),
            },
        );
    }
    if mouse.just_released(MouseButton::Left) {
        ins.mouse_held = false;
        if ins.drag.take().is_some() {
            send(&sim, Command::ClearDrag);
            live.drag = None;
        }
    }

    if let Some(drag) = ins.drag {
        let Some(b) = live.bodies.get(drag.body).copied() else {
            ins.drag = None;
            return;
        };
        let grab = {
            let r = axl_render::instruments::rotate(b.quat, drag.local);
            [b.pos[0] + r[0], b.pos[1] + r[1], b.pos[2] + r[2]]
        };
        let target = drag.fixed_target.or_else(|| {
            // The cursor ray meets the plane through the grab point that faces the camera.
            let (o, d) = cursor_ray(window, cam, gt, insets)?;
            let n = bevy_to_axl(gt.forward().as_vec3());
            let denom = n.dot(d);
            if denom.abs() < 1e-6 {
                return None;
            }
            let g = Vec3::new(grab[0] as f32, grab[1] as f32, grab[2] as f32);
            let s = n.dot(g - o) / denom;
            Some(v3(o + d * s))
        });
        if let Some(target) = target {
            let m = live.robot_mass.max(1e-3);
            // The force is set once per frame and held, so the spring must be slow compared to
            // the frame time or it blows up (a slow frame rate gets a softer spring).
            let omega = SPRING_OMEGA.min(0.5 / f64::from(time.delta_secs()).max(1e-3));
            let k = m * omega * omega;
            let c = 2.0 * SPRING_ZETA * m * omega;
            let mut f = [0.0; 3];
            for i in 0..3 {
                f[i] = k * (target[i] - grab[i]) - c * b.velocity[i];
            }
            let norm = f.iter().map(|x| x * x).sum::<f64>().sqrt();
            let cap = SPRING_MAX_WEIGHTS * m * 9.81;
            if norm > cap {
                for x in &mut f {
                    *x *= cap / norm;
                }
            }
            send(
                &sim,
                Command::ApplyDragForce {
                    body: drag.body,
                    point: grab,
                    force: f,
                },
            );
            live.drag = Some((grab, target));
        }
    }

    // ---- joint hover: the nearest hinge anchor within 16 px of the cursor ----
    if ins.drag.is_none() && !ins.script.hover_joint {
        live.hover_joint = None;
        if !ui.pointer
            && let Some(cur) = ins.cursor
        {
            let mut best = (16.0_f32, None);
            for (i, j) in live.joints.iter().enumerate() {
                if let Some(p) = to_screen(cam, gt, window, insets, j.anchor) {
                    let d = p.distance(cur);
                    if d < best.0 {
                        best = (d, Some(i));
                    }
                }
            }
            live.hover_joint = best.1;
        }
    }
}

fn fmt3(v: [f64; 3], unit: &str) -> String {
    format!("{:+.3}, {:+.3}, {:+.3} {unit}", v[0], v[1], v[2])
}

fn fmt3e(v: [f64; 3], unit: &str) -> String {
    format!("{:.2e}, {:.2e}, {:.2e} {unit}", v[0], v[1], v[2])
}

fn norm(v: [f64; 3]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Write the plot CSV: to the test hook's path when set, else where the person chooses.
fn export_csv(ins: &mut Instruments, lo: f64, hi: f64) {
    let names: Vec<String> = ins.channels_on.iter().cloned().collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let text = ins.plots.to_csv(&refs, lo, hi);
    let path = ins.csv_path.clone().or_else(|| {
        rfd::FileDialog::new()
            .set_file_name("axl-plot.csv")
            .add_filter("CSV", &["csv"])
            .save_file()
    });
    ins.csv_note = match path {
        Some(p) => match std::fs::write(&p, text) {
            Ok(()) => format!("Saved {} channel(s) to {}", names.len(), p.display()),
            Err(e) => format!("Could not save: {e}"),
        },
        None => "Export cancelled".to_owned(),
    };
}

/// The panels: overlay switches, time bar, plotter, inspector, joint tip and sensor labels.
#[allow(clippy::too_many_arguments)]
pub fn instruments_ui(
    mut contexts: EguiContexts,
    sim: Res<SandboxSim>,
    mut ins: ResMut<Instruments>,
    mut overlays: ResMut<Overlays>,
    mut live: ResMut<LiveView>,
    scene: Option<Res<SandboxScene>>,
    top: Res<TopInset>,
    insets: Option<Res<ViewInsets>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<ViewCamera>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let screen = ctx.viewport_rect();
    let free = match insets.as_ref().and_then(|i| i.rect) {
        Some([l, t, r, b]) => egui::Rect::from_min_max(egui::pos2(l, t), egui::pos2(r, b)),
        None => egui::Rect::from_min_max(
            screen.min + egui::vec2(0.0, top.0.max(28.0)),
            screen.max - egui::vec2(0.0, 26.0),
        ),
    };
    let ins = &mut *ins;
    let frame = egui::Frame::window(&ctx.global_style());

    // ---- overlay switches (top left of the free area) ----
    egui::Area::new(egui::Id::new("overlay_switches"))
        .fixed_pos(free.min + egui::vec2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            frame.show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Overlays");
                    for (i, name) in Overlays::NAMES.iter().enumerate() {
                        let mut on = overlays.get(i);
                        if ui
                            .toggle_value(&mut on, format!("{} {name}", i + 1))
                            .changed()
                        {
                            overlays.set(i, on);
                        }
                    }
                });
            });
        });

    // ---- time bar (bottom centre of the free area) ----
    let mut scrub = ins.scrub;
    let mut plot_open = ins.plot_open;
    egui::Area::new(egui::Id::new("time_bar"))
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(free.center().x, free.bottom() - 8.0))
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Reset").on_hover_text("Ctrl+R").clicked() {
                        send(&sim, Command::Reset);
                    }
                    let label = if ins.paused { "Play" } else { "Pause" };
                    if ui.button(label).on_hover_text("Space").clicked() {
                        send(
                            &sim,
                            if ins.paused {
                                Command::Resume
                            } else {
                                Command::Pause
                            },
                        );
                    }
                    for (text, ms, tip) in [("+1 ms", 1, "."), ("+10 ms", 10, "Shift+.")] {
                        if ui.button(text).on_hover_text(tip).clicked() {
                            if !ins.paused {
                                send(&sim, Command::Pause);
                            }
                            send(&sim, Command::Step { ms });
                        }
                    }
                    if ui.small_button("<").on_hover_text("[").clicked() {
                        send(&sim, Command::SetSpeed(next_speed(ins.speed, false)));
                    }
                    ui.label(format!("{}x", ins.speed));
                    if ui.small_button(">").on_hover_text("]").clicked() {
                        send(&sim, Command::SetSpeed(next_speed(ins.speed, true)));
                    }
                    ui.separator();
                    ui.monospace(format!("t = {:7.3} s", ins.time));
                    let mut v = scrub.unwrap_or(ins.time);
                    ui.spacing_mut().slider_width = 220.0;
                    let r = ui.add(
                        egui::Slider::new(&mut v, 0.0..=ins.max_time.max(1.0)).show_value(false),
                    );
                    if r.drag_started() && scrub.is_none() {
                        send(&sim, Command::Pause);
                    }
                    if r.changed() {
                        scrub = Some(v);
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        send(&sim, Command::Rewind(v));
                        scrub = None;
                    }
                    if let Some(t) = ins.replay {
                        ui.label(format!("replaying to {t:.1} s"));
                    }
                    ui.separator();
                    ui.checkbox(&mut plot_open, "Plots");
                });
            });
        });
    ins.scrub = scrub;
    ins.plot_open = plot_open;

    // ---- plotter ----
    if ins.plot_open {
        plotter(&ctx, free, ins);
    }

    // ---- inspector ----
    if let Some(sel) = live.selected {
        let mut open = true;
        let name = scene
            .as_ref()
            .and_then(|s| s.bodies.bodies.iter().find(|b| b.id == sel))
            .map_or_else(|| format!("body {sel}"), |b| b.name.clone());
        let b = live.bodies.get(sel).copied();
        egui::Window::new("Inspector")
            .open(&mut open)
            .default_pos(egui::pos2(free.right() - 300.0, free.top() + 52.0))
            .resizable(false)
            .show(&ctx, |ui| {
                ui.strong(&name);
                if let Some(b) = b {
                    egui::Grid::new("inspector_grid").show(ui, |ui| {
                        ui.label("Mass");
                        ui.label(format!("{:.1} g", b.mass * 1000.0));
                        ui.end_row();
                        ui.label("Centre of mass");
                        ui.label(fmt3(b.com, "m"));
                        ui.end_row();
                        ui.label("Inertia");
                        ui.label(fmt3e(b.inertia, "kg m^2"));
                        ui.end_row();
                        ui.label("Velocity");
                        ui.label(format!(
                            "{} ({:.3} m/s)",
                            fmt3(b.velocity, "m/s"),
                            norm(b.velocity)
                        ));
                        ui.end_row();
                        ui.label("Spin");
                        ui.label(fmt3(b.angular_velocity, "rad/s"));
                        ui.end_row();
                    });
                }
            });
        if !open {
            live.selected = None;
        }
    }

    // ---- joint hover tip and sensor labels ----
    let (Ok(window), Ok((cam, gt))) = (windows.single(), cams.single()) else {
        return Ok(());
    };
    let insets = insets.as_deref();
    if let Some(j) = live.hover_joint.and_then(|i| live.joints.get(i)).copied()
        && let Some(cur) = ins.cursor
    {
        egui::Area::new(egui::Id::new("joint_tip"))
            .fixed_pos(egui::pos2(cur.x + 16.0, cur.y + 16.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(&ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.strong("Hinge");
                    ui.label(format!("angle {:+.1} deg", j.angle.to_degrees()));
                    ui.label(format!("speed {:+.2} rad/s", j.speed));
                    ui.label(format!("torque {:+.4} N m", j.torque));
                });
            });
    }
    if overlays.sensors {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("sensor_labels"),
        ));
        for s in &live.sensors {
            let tip = [
                s.origin[0] + s.axis[0] * s.range * 0.5,
                s.origin[1] + s.axis[1] * s.range * 0.5,
                s.origin[2] + s.axis[2] * s.range * 0.5 + 0.01,
            ];
            if let Some(p) = to_screen(cam, gt, window, insets, tip) {
                let pos = egui::pos2(p.x, p.y);
                let galley = painter.layout_no_wrap(
                    s.label.clone(),
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
                painter.rect_filled(
                    egui::Rect::from_min_size(pos, galley.size()).expand(3.0),
                    3.0,
                    egui::Color32::from_black_alpha(190),
                );
                painter.galley(pos, galley, egui::Color32::WHITE);
            }
        }
    }
    Ok(())
}

fn plotter(ctx: &egui::Context, free: egui::Rect, ins: &mut Instruments) {
    use egui_plot::{Legend, Line, Plot, PlotPoints, VLine};
    let mut open = true;
    let right = ins.frozen.unwrap_or(ins.time);
    let lo = (right - ins.window_s).max(0.0);
    let hi = lo + ins.window_s;
    let on: Vec<String> = ins.channels_on.iter().cloned().collect();
    let data: Vec<(String, Vec<[f64; 2]>)> = on
        .iter()
        .map(|n| {
            (
                n.clone(),
                ins.plots
                    .window(n, lo, hi)
                    .into_iter()
                    .map(|(t, v)| [t, v])
                    .collect(),
            )
        })
        .collect();
    let mut cursor_t = None;
    let mut export = false;
    egui::Window::new("Plots")
        .open(&mut open)
        .default_pos(egui::pos2(free.right() - 580.0, free.bottom() - 340.0))
        .default_size(egui::vec2(560.0, 250.0))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Window");
                egui::ComboBox::from_id_salt("plot_window")
                    .selected_text(format!("{} s", ins.window_s))
                    .show_ui(ui, |ui| {
                        for w in WINDOWS {
                            ui.selectable_value(&mut ins.window_s, w, format!("{w} s"));
                        }
                    });
                let mut frozen = ins.frozen.is_some();
                if ui.checkbox(&mut frozen, "Pause plot").changed() {
                    ins.frozen = frozen.then_some(ins.time);
                }
                if ui.button("Export CSV").clicked() {
                    export = true;
                }
                if !ins.csv_note.is_empty() {
                    ui.weak(&ins.csv_note);
                }
            });
            ui.horizontal_wrapped(|ui| {
                let all: Vec<(String, bool)> = ins
                    .plots
                    .series()
                    .iter()
                    .map(|s| (s.name.clone(), s.builtin))
                    .collect();
                for (name, builtin) in all {
                    let mut is_on = ins.channels_on.contains(&name);
                    let text = if builtin {
                        egui::RichText::new(&name)
                    } else {
                        egui::RichText::new(&name).strong()
                    };
                    if ui.checkbox(&mut is_on, text).changed() {
                        if is_on {
                            ins.channels_on.insert(name);
                        } else {
                            ins.channels_on.remove(&name);
                        }
                    }
                }
            });
            let readout = match ins.plot_cursor {
                Some(t) => {
                    let vals: Vec<String> = on
                        .iter()
                        .filter_map(|n| ins.plots.value_at(n, t).map(|v| format!("{n} = {v:.4}")))
                        .collect();
                    format!("t = {t:.3} s   {}", vals.join("   "))
                }
                None => "Hover the plot to read values".to_owned(),
            };
            ui.monospace(readout);
            let resp = Plot::new("axl_plot")
                .height(150.0)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .legend(Legend::default())
                .x_axis_label("simulated time (s)")
                .show(ui, move |p| {
                    for (name, pts) in data {
                        p.line(Line::new(name, PlotPoints::from(pts)));
                    }
                    p.set_plot_bounds_x(lo..=hi);
                    let c = p.pointer_coordinate();
                    if let Some(c) = c {
                        p.vline(VLine::new("", c.x));
                    }
                    c.map(|c| c.x)
                });
            cursor_t = if resp.response.hovered() {
                resp.inner
            } else {
                None
            };
        });
    ins.plot_cursor = cursor_t;
    if export {
        export_csv(ins, lo, hi);
    }
    // Scripted export.
    if let Some(at) = ins.script.export_at
        && ins.time >= at
    {
        ins.script.export_at = None;
        export_csv(ins, 0.0, ins.time);
    }
    ins.plot_open = open;
}

/// Parse the scripted actions of `--instruments` (see the flag) into a [`Script`] and the overlays to switch on.
pub fn parse_script(spec: &str) -> Result<(Script, Vec<usize>), String> {
    let mut s = Script::default();
    let mut overlays = Vec::new();
    let num = |v: &str| {
        v.trim()
            .parse::<f64>()
            .map_err(|_| format!("{v:?} is not a number"))
    };
    for item in spec.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        let (k, v) = item.split_once('=').unwrap_or((item, ""));
        match k {
            "overlays" => {
                for n in v.split('+').filter(|n| !n.is_empty()) {
                    let n: usize = n
                        .parse()
                        .map_err(|_| format!("{n:?} is not an overlay number"))?;
                    if !(1..=7).contains(&n) {
                        return Err(format!("overlay {n} does not exist (1 to 7)"));
                    }
                    overlays.push(n - 1);
                }
            }
            "plots" => s.plots = true,
            "select" => s.select = Some(num(v)? as usize),
            "pause_at" => s.pause_at = Some(num(v)?),
            "export_at" => s.export_at = Some(num(v)?),
            "drag_at" => s.drag_at = Some(num(v)?),
            "hover_joint" => s.hover_joint = true,
            "rewind" => {
                let (a, b) = v.split_once(':').ok_or("rewind needs AT:TO")?;
                s.rewind = Some((num(a)?, num(b)?));
            }
            other => return Err(format!("unknown instrument action {other:?}")),
        }
    }
    Ok((s, overlays))
}

/// Add the instruments to the app.
pub fn install(app: &mut App, script: Script, switch_on: &[usize], collect_inbox: bool) {
    let mut ins = Instruments::default();
    {
        let mut o = app.world_mut().resource_mut::<Overlays>();
        for &i in switch_on {
            o.set(i, true);
        }
    }
    ins.plot_open = script.plots;
    ins.collect_inbox = collect_inbox;
    ins.script = script;
    app.insert_resource(ins).add_systems(
        Update,
        (instruments_tick, instruments_input)
            .chain()
            .before(axl_render::sync::sync_bodies),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_keys_step_through_the_list_and_stop_at_the_ends() {
        assert_eq!(next_speed(1.0, true), 2.0);
        assert_eq!(next_speed(1.0, false), 0.5);
        assert_eq!(next_speed(4.0, true), 4.0);
        assert_eq!(next_speed(0.1, false), 0.1);
        // An odd value snaps to the nearest notch first.
        assert_eq!(next_speed(0.9, true), 2.0);
    }

    #[test]
    fn the_exported_csv_parses_back() {
        let mut p = PlotStore::new();
        for i in 0..10 {
            let t = f64::from(i) * 0.1;
            p.push("a, b", false, t, t * 2.0);
            p.push("c", true, t, -t);
        }
        let csv = p.to_csv(&["a, b", "c"], 0.2, 0.6);
        let parsed = axl_sim::plot::parse_csv(&csv).unwrap();
        assert_eq!(parsed.channels, vec!["a, b".to_owned(), "c".to_owned()]);
        assert_eq!(parsed.rows.len(), 5);
        assert!((parsed.rows[0].0 - 0.2).abs() < 1e-12);
        assert_eq!(parsed.rows[0].1[0], Some(0.4));
    }
}
