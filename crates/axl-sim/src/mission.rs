//! Missions on the sim thread's stepping core: the world and robot of a mission, a recorder that
//! samples a run every 10 ms of simulated time, and the unpaced graded run over the grading seeds
//! (`axl --headless --mission mNN --controller robot.wasm`).
//!
//! A graded run is deterministic: the same program, robot, mission and seed give the same trace on
//! the same build. Wall-clock time plays no part, only the watchdog (a program that runs for 2 s
//! without calling `axl_step`) uses it.

use std::collections::BTreeMap;
use std::path::Path;

use axl_build::model::Assembly;
use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_host::{HostConfig, RunEnd};
use axl_hw::{HwOptions, MatTexture, MotorCommand};
use axl_missions::{
    Criterion, EvalContext, Expected, MissionDef, RobotSource, RunTrace, Sample, grade,
};
use axl_mujoco::{BodyId, ContactInfo, GeomId, GroupMask};
use axl_parts::catalog::Catalog;
use axl_world::{WorldFriction, build_world, builtin_objects, builtin_worlds};

use crate::program::{Headless, run_unpaced};
use crate::{Push, Scenario, SimCore, SimError};

pub use crate::program::ProgramConfig;

/// Simulated seconds between trace samples.
pub const SAMPLE_INTERVAL_MS: u64 = 10;
/// Wheel speed of the runner's own measuring drives, rad/s: slow enough that the tyres do not slip.
const GENTLE_RAD_S: f64 = 3.0;
/// Height above the floor of the ray that measures the distance ahead, metres (mid-height of the
/// 100 mm walls and obstacles).
const AHEAD_RAY_Z_M: f64 = 0.04;

/// Why a mission could not be run.
#[derive(Debug, thiserror::Error)]
pub enum MissionRunError {
    /// No such mission, world or variant, or a bad world file.
    #[error("{0}")]
    World(String),
    /// The robot could not be built or compiled.
    #[error("{0}")]
    Robot(String),
    /// The simulation could not start.
    #[error(transparent)]
    Sim(#[from] SimError),
}

/// Everything a run of one mission on one seed needs, except the controller.
#[derive(Clone, Debug)]
pub struct MissionSetup {
    /// The robot inside the world, compiled.
    pub compiled: Compiled,
    /// The mat, the pushes and the time limit.
    pub scenario: Scenario,
    /// Zones and the guide line for the criteria.
    pub eval: EvalContext,
    /// Hardware options (seed and start charge).
    pub hw: HwOptions,
    /// Names of the geoms that count as walls for contact tagging.
    pub walls: Vec<String>,
    /// MuJoCo ids of the robot's bodies.
    pub robot_bodies: Vec<usize>,
    /// Parts in the robot.
    pub part_count: u16,
    /// Hinge joints of the drive motors (their bodies give the point the robot turns about).
    pub pivot_joints: Vec<String>,
    /// Spawn pose.
    pub spawn: axl_world::Pose2,
}

fn friction() -> WorldFriction {
    let p = PhysicsParams::builtin();
    WorldFriction {
        wall: p.friction.wall,
        plastic: p.friction.abs,
        rubber: p.friction.rubber_sliding,
    }
}

/// The robot a mission starts with: its starter, or the learner's design from `file`.
pub fn robot_assembly(
    m: &MissionDef,
    cat: &Catalog,
    file: Option<&Path>,
) -> Result<Assembly, MissionRunError> {
    match (&m.robot, file) {
        (_, Some(path)) => {
            axl_build::io::load(path, cat).map_err(|e| MissionRunError::Robot(e.to_string()))
        }
        (RobotSource::Starter(id), None) => {
            build_starter(cat, id).map_err(|e| MissionRunError::Robot(e.to_string()))
        }
        (RobotSource::Build { .. }, None) => Err(MissionRunError::Robot(format!(
            "Mission {} needs a robot you built: pass a .axlbot file.",
            m.id
        ))),
    }
}

/// Build the world and compile the robot into it for `seed`.
pub fn setup(
    m: &MissionDef,
    seed: u64,
    asm: &Assembly,
    cat: &Catalog,
) -> Result<MissionSetup, MissionRunError> {
    let worlds = builtin_worlds().map_err(|e| MissionRunError::World(e.to_string()))?;
    let objects = builtin_objects().map_err(|e| MissionRunError::World(e.to_string()))?;
    let def = worlds.get(&m.world.id).ok_or_else(|| {
        MissionRunError::World(format!(
            "mission {} uses unknown world {}",
            m.id, m.world.id
        ))
    })?;
    let built = build_world(def, m.world.variant_for(seed), seed, &objects, &friction())
        .map_err(MissionRunError::World)?;
    // The robot frame's origin is the hub, which need not be where the learner put the origin of
    // the design: the spawn point is where the hub goes.
    let hub = asm
        .parts
        .iter()
        .find(|p| p.part.0 == "hub")
        .map_or([0.0, 0.0], |p| {
            // Placement is in 0.1 mm steps.
            [f64::from(p.pos[0]) * 1e-4, f64::from(p.pos[1]) * 1e-4]
        });
    let (sy, cy) = built.spawn.yaw.sin_cos();
    let compiled = compile(
        cat,
        asm,
        &PhysicsParams::builtin(),
        &CompileOptions {
            spawn_xy: [
                built.spawn.x - (cy * hub[0] - sy * hub[1]),
                built.spawn.y - (sy * hub[0] + cy * hub[1]),
            ],
            spawn_yaw: built.spawn.yaw,
            extra_worldbody: built.worldbody.clone(),
        },
    )
    .map_err(|e| MissionRunError::Robot(e.to_string()))?;
    let floor = built.mat.as_ref().map(|r| MatTexture {
        min: r.min,
        max: r.max,
        width: r.width,
        height: r.height,
        pixels: r.pixels.clone(),
    });
    let pushes = m
        .events
        .iter()
        .map(|e| match e {
            axl_missions::MissionEvent::Push { t, impulse_ns } => Push {
                t: *t,
                impulse_ns: *impulse_ns,
            },
        })
        .collect();
    let robot_bodies = compiled.bodies.bodies.iter().map(|b| b.id).collect();
    let pivot_joints = compiled
        .devices
        .devices
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                axl_parts::types::DeviceModel::MotorMedium
                    | axl_parts::types::DeviceModel::MotorLarge
            )
        })
        .filter_map(|d| d.joint.as_ref().map(|j| j.name.clone()))
        .collect();
    let mut walls = built.wall_geoms.clone();
    walls.extend(built.obstacle_geoms.iter().cloned());
    Ok(MissionSetup {
        robot_bodies,
        pivot_joints,
        walls,
        part_count: u16::try_from(asm.parts.len()).unwrap_or(u16::MAX),
        eval: EvalContext {
            zones: built.zones.clone(),
            line_path: built.guide_path.clone(),
        },
        hw: HwOptions {
            seed,
            noise: true,
            start_soc: Some(m.soc_for(seed)),
        },
        scenario: Scenario {
            floor,
            pushes,
            limit_s: Some(m.time_limit_s),
        },
        spawn: built.spawn,
        compiled,
    })
}

impl MissionSetup {
    /// A fresh stepping core for this setup, at time 0.
    pub fn core(&self) -> Result<SimCore, SimError> {
        let mut core = SimCore::with_options(&self.compiled, &[], self.hw)?;
        core.set_scenario(&self.scenario);
        core.reset();
        Ok(core)
    }

    /// The recorder for this setup.
    pub fn recorder(&self) -> Recorder {
        let mut r = Recorder::new(
            self.seed(),
            self.part_count,
            &self.walls,
            self.robot_bodies.clone(),
        );
        r.pivot_joints = self.pivot_joints.clone();
        r
    }

    /// The run seed.
    pub fn seed(&self) -> u64 {
        self.hw.seed
    }
}

pub(crate) fn yaw_of(q: [f64; 4]) -> f64 {
    (2.0 * (q[0] * q[3] + q[1] * q[2])).atan2(1.0 - 2.0 * (q[2] * q[2] + q[3] * q[3]))
}

/// Samples a run every 10 ms and tags wall contacts every tick.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    seed: u64,
    part_count: u16,
    wall_names: Vec<String>,
    walls: Vec<GeomId>,
    robot_bodies: Vec<usize>,
    pivot_joints: Vec<String>,
    pivot_bodies: Vec<usize>,
    root: usize,
    front_off: f64,
    next_ms: u64,
    last_raw_yaw: f64,
    yaw: f64,
    wall_hit: bool,
    touching: bool,
    energy_ws: f64,
    collisions: u32,
    contacts: Vec<ContactInfo>,
    /// The samples.
    pub samples: Vec<Sample>,
    /// `axl_report` calls: (time, key, value).
    pub reports: Vec<(f64, String, f64)>,
    /// `axl_mark_pose` calls.
    pub believed: Vec<(f64, [f64; 3])>,
    /// `axl_plot` was called.
    pub plot_used: bool,
}

impl Recorder {
    /// A recorder; [`Recorder::start`] finishes it once the core exists.
    pub fn new(seed: u64, part_count: u16, walls: &[String], robot_bodies: Vec<usize>) -> Self {
        Self {
            seed,
            part_count,
            wall_names: walls.to_vec(),
            robot_bodies,
            ..Self::default()
        }
    }

    /// Find the wall geoms and the front of the robot (the most forward point of any robot geom,
    /// found with rays shot back at the robot along its heading).
    pub fn start(&mut self, core: &mut SimCore) {
        self.root = core.root_body().0;
        self.pivot_bodies = self
            .pivot_joints
            .iter()
            .filter_map(|n| core.physics().joint_id(n).ok())
            .filter_map(|j| core.physics().joint_body(j))
            .map(|b| b.0)
            .collect();
        self.walls = self
            .wall_names
            .iter()
            .filter_map(|n| core.physics().geom_id(n).ok())
            .collect();
        let physics = core.physics_mut();
        physics.forward();
        let pose = physics.body_pose(BodyId(self.root));
        let yaw = yaw_of(pose.quat);
        self.last_raw_yaw = yaw;
        self.yaw = yaw;
        let (s, c) = yaw.sin_cos();
        let mut nearest = f64::INFINITY;
        const START: f64 = 1.0;
        for iy in -30..=30 {
            for iz in 1..=40 {
                let lat = f64::from(iy) * 0.005;
                let z = f64::from(iz) * 0.005;
                let from = [
                    pose.pos[0] + c * START - s * lat,
                    pose.pos[1] + s * START + c * lat,
                    z,
                ];
                if let Some(h) = physics.ray(from, [-c, -s, 0.0], GroupMask(0b10)) {
                    nearest = nearest.min(h.distance);
                }
            }
        }
        self.front_off = if nearest.is_finite() {
            START - nearest
        } else {
            0.0
        };
        self.next_ms = 0;
        self.sample(core);
        self.next_ms = SAMPLE_INTERVAL_MS;
    }

    /// Call after every tick.
    pub fn after_tick(&mut self, core: &mut SimCore) {
        let hw = core.hardware().battery();
        self.energy_ws += hw.voltage() * hw.current() * core.physics().timestep();
        if !self.walls.is_empty() {
            core.physics().contacts_into(&mut self.contacts);
            let mut touching = false;
            for c in &self.contacts {
                let (a, b) = (c.geom1, c.geom2);
                let wall = |g: GeomId| self.walls.contains(&g);
                let robot = |g: GeomId| {
                    core.physics()
                        .geom_body(g)
                        .is_some_and(|b| self.robot_bodies.contains(&b.0))
                };
                if (wall(a) && robot(b)) || (wall(b) && robot(a)) {
                    touching = true;
                    break;
                }
            }
            if touching {
                self.wall_hit = true;
                if !self.touching {
                    self.collisions += 1;
                }
            }
            self.touching = touching;
        }
        let ms = (core.time() * 1000.0).round() as u64;
        if ms >= self.next_ms {
            self.sample(core);
            self.next_ms = ms - ms % SAMPLE_INTERVAL_MS + SAMPLE_INTERVAL_MS;
        }
    }

    fn sample(&mut self, core: &mut SimCore) {
        let time = core.time();
        let physics = core.physics_mut();
        let pose = physics.body_pose(BodyId(self.root));
        let raw = yaw_of(pose.quat);
        let mut d = raw - self.last_raw_yaw;
        d = (d + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        self.yaw += d;
        self.last_raw_yaw = raw;
        let q = pose.quat;
        let up_z = 1.0 - 2.0 * (q[1] * q[1] + q[2] * q[2]);
        let v = physics.body_linear_velocity(BodyId(self.root));
        let (s, c) = raw.sin_cos();
        let pivot = if self.pivot_bodies.is_empty() {
            [pose.pos[0], pose.pos[1]]
        } else {
            let n = self.pivot_bodies.len() as f64;
            let sum = self.pivot_bodies.iter().fold([0.0, 0.0], |a, &b| {
                let p = physics.body_pose(BodyId(b)).pos;
                [a[0] + p[0], a[1] + p[1]]
            });
            [sum[0] / n, sum[1] / n]
        };
        let front = [
            pose.pos[0] + c * self.front_off,
            pose.pos[1] + s * self.front_off,
        ];
        const BACK: f64 = 0.005;
        let ahead_m = physics
            .ray(
                [front[0] - c * BACK, front[1] - s * BACK, AHEAD_RAY_Z_M],
                [c, s, 0.0],
                GroupMask(0b1),
            )
            .map_or(-1.0, |h| (h.distance - BACK).max(0.0));
        self.samples.push(Sample {
            t: time,
            pos: pose.pos,
            yaw: self.yaw,
            tilt: up_z.clamp(-1.0, 1.0).acos(),
            speed: (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(),
            front,
            ahead_m,
            wall_contact: std::mem::take(&mut self.wall_hit),
            pivot,
        });
    }

    /// The trace so far.
    pub fn into_trace(self) -> RunTrace {
        RunTrace {
            seed: self.seed,
            samples: self.samples,
            reports: self.reports,
            measured: BTreeMap::new(),
            believed: self.believed,
            plot_used: self.plot_used,
            part_count: self.part_count,
            program_error: None,
            unstable: false,
            energy_wh: self.energy_ws / 3600.0,
            collisions: self.collisions,
        }
    }
}

/// One graded run.
#[derive(Clone, Debug)]
pub struct SeedRun {
    /// What was recorded.
    pub trace: RunTrace,
    /// Lines the program printed.
    pub logs: Vec<String>,
}

/// Run `wasm` on `seed` as fast as possible and return its trace. `measured` is added to the trace.
pub fn run_seed(
    m: &MissionDef,
    asm: &Assembly,
    cat: &Catalog,
    seed: u64,
    wasm: Option<&[u8]>,
    measured: &BTreeMap<String, f64>,
) -> Result<(SeedRun, EvalContext), MissionRunError> {
    let s = setup(m, seed, asm, cat)?;
    let core = s.core()?;
    let program = wasm.map(|w| ProgramConfig {
        wasm: w.to_vec(),
        host: HostConfig {
            disabled_api: m.disabled_api.clone(),
            seed,
            ..HostConfig::default()
        },
    });
    let Headless {
        recorder,
        end,
        unstable,
        logs,
    } = run_unpaced(core, program.as_ref(), s.recorder());
    let mut trace = recorder.into_trace();
    trace.measured = measured.clone();
    trace.unstable = unstable;
    trace.program_error = match end {
        Some(Ok(RunEnd::Stopped(e))) => Some(e.to_string()),
        Some(Err(e)) => Some(e),
        _ => None,
    };
    Ok((SeedRun { trace, logs }, s.eval))
}

/// The keys of the numbers the runner must measure for a mission.
pub fn needed_keys(m: &MissionDef) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(p) = &m.predict
        && p.measured != "final_x"
        && p.measured != "final_y"
    {
        keys.push(p.measured.clone());
    }
    for c in m
        .criteria
        .iter()
        .chain(m.stars.iter().flat_map(|r| r.criteria.iter()))
    {
        if let Criterion::Report {
            expected: Expected::Measured(k),
            ..
        } = c
        {
            keys.push(k.clone());
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

fn travelled(core: &SimCore, from: [f64; 3]) -> f64 {
    let p = core.physics().body_pose(core.root_body()).pos;
    (p[0] - from[0]).hypot(p[1] - from[1])
}

fn run_secs(core: &mut SimCore, secs: f64) {
    let end = core.time() + secs;
    while core.time() < end - 1e-9 {
        if !core.tick() {
            break;
        }
    }
}

fn drive(core: &mut SimCore, left: MotorCommand, right: MotorCommand) {
    let _ = core.command("left", left);
    let _ = core.command("right", right);
}

/// Measure one named number in the mission's world (see the mission files for the keys). `None`
/// when the key is unknown or the robot lacks the device it needs.
pub fn measure(
    key: &str,
    m: &MissionDef,
    asm: &Assembly,
    cat: &Catalog,
) -> Result<Option<f64>, MissionRunError> {
    let seed = match key {
        "sees_angled_wall" => m.world.seed_variants.first().map_or(1, |(s, _)| *s),
        _ => 1,
    };
    let s = setup(m, seed, asm, cat)?;
    let mut core = s.core()?;
    let turn = std::f64::consts::TAU;
    let value = match key {
        "drive_half_power_2s_mm" => {
            let p0 = core.physics().body_pose(core.root_body()).pos;
            drive(
                &mut core,
                MotorCommand::Power(0.5),
                MotorCommand::Power(0.5),
            );
            run_secs(&mut core, 2.0);
            Some(travelled(&core, p0) * 1000.0)
        }
        "one_turn_mm" => {
            let p0 = core.physics().body_pose(core.root_body()).pos;
            let go = MotorCommand::Position {
                target: turn,
                max_speed: GENTLE_RAD_S,
            };
            drive(&mut core, go, go);
            run_secs(&mut core, 4.0);
            Some(travelled(&core, p0) * 1000.0)
        }
        "spin_one_turn_deg" => {
            let y0 = yaw_of(core.physics().body_pose(core.root_body()).quat);
            drive(
                &mut core,
                MotorCommand::Position {
                    target: turn,
                    max_speed: GENTLE_RAD_S,
                },
                MotorCommand::Position {
                    target: -turn,
                    max_speed: GENTLE_RAD_S,
                },
            );
            run_secs(&mut core, 4.0);
            let y1 = yaw_of(core.physics().body_pose(core.root_body()).quat);
            let d = (y1 - y0 + std::f64::consts::PI).rem_euclid(turn) - std::f64::consts::PI;
            Some(d.abs().to_degrees())
        }
        "turn_error_square_mm" => {
            // Four sides of 0.5 m, every turn 2 degrees short.
            let (mut x, mut y, mut h) = (0.0_f64, 0.0_f64, 0.0_f64);
            for _ in 0..4 {
                x += 0.5 * h.cos();
                y += 0.5 * h.sin();
                h += 88.0_f64.to_radians();
            }
            Some(x.hypot(y) * 1000.0)
        }
        "sees_angled_wall" | "sensor_range_m" | "sensor_sigma_mm" | "reflect_on_line" => {
            run_secs(&mut core, 0.3);
            sensor_measure(key, &mut core)
        }
        "ramp_speed_half_power_ms" => {
            drive(
                &mut core,
                MotorCommand::Power(0.5),
                MotorCommand::Power(0.5),
            );
            let (mut sum, mut n) = (0.0, 0);
            while core.time() < 5.0 - 1e-9 {
                run_secs(&mut core, 0.01);
                if core.time() >= 2.0 {
                    let v = core.physics().body_linear_velocity(core.root_body());
                    sum += (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    n += 1;
                }
            }
            Some(sum / f64::from(n.max(1)))
        }
        "lap_time_at_015_s" => s.eval.line_path.as_ref().map(|p| {
            let len: f64 = p
                .windows(2)
                .map(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]))
                .sum();
            len / 0.15
        }),
        "open_loop_drift_mm" => {
            let y0 = core.physics().body_pose(core.root_body()).pos[1];
            drive(
                &mut core,
                MotorCommand::Power(0.5),
                MotorCommand::Power(0.5),
            );
            run_secs(&mut core, 3.0);
            let y1 = core.physics().body_pose(core.root_body()).pos[1];
            Some((y1 - y0).abs() * 1000.0)
        }
        "shortest_path_cells" => {
            let worlds = builtin_worlds().map_err(|e| MissionRunError::World(e.to_string()))?;
            worlds
                .get(&m.world.id)
                .and_then(|w| w.maze.as_ref())
                .and_then(|mz| {
                    let idx = (seed.wrapping_sub(1) % mz.layout_seeds.len() as u64) as usize;
                    axl_world::maze::generate(mz.cells, mz.layout_seeds[idx]).shortest_path()
                })
                .map(|n| n as f64)
        }
        "hold_force_n" => {
            // A 1 kg robot on the mission's ramp: m g sin(angle) along the slope.
            let angle = match m.world.variant.as_deref() {
                Some("ramp_25") => 25.0_f64,
                Some("ramp_20") => 20.0,
                Some("ramp_15") => 15.0,
                _ => 10.0,
            };
            Some(core.physics().gravity()[2].abs() * angle.to_radians().sin())
        }
        _ => None,
    };
    Ok(value)
}

fn sensor_measure(key: &str, core: &mut SimCore) -> Option<f64> {
    use axl_hw::Slot;
    let devices: Vec<(String, Slot)> = core
        .hardware()
        .devices()
        .iter()
        .map(|d| (d.name.clone(), d.slot))
        .collect();
    let first_range = devices
        .iter()
        .find(|(_, s)| matches!(s, Slot::Ultrasonic(_) | Slot::Tof(_)))
        .map(|(_, s)| *s);
    match key {
        "sees_angled_wall" => {
            let d = core.hardware().distance(first_range?)?;
            Some(if d > 0.0 { 1.0 } else { 0.0 })
        }
        "sensor_range_m" => {
            let mut vis = Vec::new();
            core.physics_mut().forward();
            let (hw, physics) = (core.hardware().clone(), core.physics_mut());
            hw.sensor_visuals(physics, &mut vis);
            let v = vis.iter().find(|v| {
                matches!(
                    v.kind,
                    axl_hw::VisualKind::Ultrasonic | axl_hw::VisualKind::Tof
                )
            })?;
            physics
                .ray(v.origin, v.axis, GroupMask::WORLD_AND_OBJECTS)
                .map(|h| h.distance)
        }
        "sensor_sigma_mm" => {
            let slot = first_range?;
            let mut v = Vec::new();
            for _ in 0..40 {
                run_secs(core, 0.05);
                if let Some(d) = core.hardware().distance(slot)
                    && d > 0.0
                {
                    v.push(d * 1000.0);
                }
            }
            if v.len() < 2 {
                return None;
            }
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            Some((v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt())
        }
        "reflect_on_line" => {
            let i = devices.iter().find_map(|(_, s)| match s {
                Slot::Color(i) => Some(*i),
                _ => None,
            })?;
            let hw = core.hardware();
            hw.color_at(i)
                .map(|c| c.reading(hw.now_ms()).reflect.round())
        }
        _ => None,
    }
}

/// Measure every number the mission needs.
pub fn measure_all(
    m: &MissionDef,
    asm: &Assembly,
    cat: &Catalog,
) -> Result<BTreeMap<String, f64>, MissionRunError> {
    let mut out = BTreeMap::new();
    for k in needed_keys(m) {
        if let Some(v) = measure(&k, m, asm, cat)? {
            out.insert(k, v);
        }
    }
    Ok(out)
}

/// A mission graded over its seeds.
#[derive(Clone, Debug)]
pub struct GradedRun {
    /// The verdict.
    pub grade: axl_missions::eval::Grade,
    /// One run per seed.
    pub runs: Vec<SeedRun>,
}

/// Run `wasm` on each of `seeds` (default: the mission's grading seeds) and grade the traces.
/// `wasm` of `None` runs a robot that does nothing.
pub fn grade_mission(
    m: &MissionDef,
    asm: &Assembly,
    cat: &Catalog,
    wasm: Option<&[u8]>,
    seeds: &[u64],
    hints_used: bool,
) -> Result<GradedRun, MissionRunError> {
    let measured = measure_all(m, asm, cat)?;
    let mut runs = Vec::new();
    let mut ctx = EvalContext::default();
    for &seed in seeds {
        let (run, c) = run_seed(m, asm, cat, seed, wasm, &measured)?;
        if runs.is_empty() {
            ctx = c;
        }
        runs.push(run);
    }
    let traces: Vec<RunTrace> = runs.iter().map(|r| r.trace.clone()).collect();
    Ok(GradedRun {
        grade: grade(m, &ctx, &traces, hints_used),
        runs,
    })
}
