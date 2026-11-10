//! axl-sim: the sim thread. It owns MuJoCo and steps it in lockstep at 1 ms, paced against the
//! wall clock, and publishes a [`SimSnapshot`] of every body pose (plus what the overlays and
//! instruments draw) through a triple buffer.
//!
//! The step loop is `hw.pre_step`, `physics.step`, `hw.post_step` (battery, motors, encoders and
//! sensors from `axl-hw`). With a controller (M7) the program's `axl_step` runs the ticks; without
//! one the thread steps on its own. Time controls (pause, step, speed, reset, rewind by replay)
//! and the mouse drag are commands ([`Command`]); see [`plot`] for the plotter's data and CSV.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod mission;
pub mod plot;
mod program;
pub use program::{MAX_PLOT_CHANNELS, ProgramConfig, SimEvent, SimRobot};

use std::sync::Arc;
use std::thread::JoinHandle;

use axl_compile::Compiled;
use axl_hw::{Hardware, HwParams, SensorVisual};
pub use axl_hw::{HwOptions, MotorCommand};
use axl_mujoco::{BodyId, ContactInfo, GeomId, JointId, Physics, PhysicsError, Pose};
use crossbeam_channel::{Receiver, Sender, unbounded};
use triple_buffer::{Output, triple_buffer};

/// Simulated seconds between published snapshots (250 Hz cap).
pub const SNAPSHOT_INTERVAL_S: f64 = 0.004;
/// The message shown when the simulation blows up (SPEC, "The step loop").
pub const UNSTABLE_MESSAGE: &str = "The simulation became unstable. This usually means a part is very light or a joint is overloaded.";

/// Why the sim could not start.
#[derive(Debug, thiserror::Error)]
pub enum SimError {
    /// MuJoCo refused the model.
    #[error("{0}")]
    Physics(#[from] PhysicsError),
    /// A motor name in the voltages is not a motor of the robot.
    #[error("There is no motor called {0}.")]
    NoMotor(String),
}

/// A moving body as the inspector shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyView {
    /// Linear velocity of the centre of mass, world frame, m/s.
    pub velocity: [f64; 3],
    /// Angular velocity, world frame, rad/s.
    pub angular_velocity: [f64; 3],
    /// Centre of mass, world frame, metres.
    pub com: [f64; 3],
    /// Mass, kg (this body only).
    pub mass: f64,
    /// Principal moments of inertia, kg m^2.
    pub inertia: [f64; 3],
}

/// A hinge as the joint hover shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointView {
    /// Joint id in the model.
    pub joint: usize,
    /// The body it moves.
    pub body: usize,
    /// Anchor, world frame, metres.
    pub anchor: [f64; 3],
    /// Unit axis, world frame.
    pub axis: [f64; 3],
    /// Angle, radians.
    pub angle: f64,
    /// Speed, rad/s.
    pub speed: f64,
    /// Torque along the axis (actuator, damping, friction and constraints), N m.
    pub torque: f64,
}

/// One contact as the contacts overlay shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactView {
    /// Position, world frame.
    pub pos: [f64; 3],
    /// Unit normal, world frame (from the first geom to the second).
    pub normal: [f64; 3],
    /// Normal force, newtons.
    pub normal_force: f64,
    /// Friction forces along the two tangents, newtons.
    pub friction: [f64; 2],
    /// Sliding friction coefficient of the pair (for the friction cone).
    pub mu: f64,
    /// A contact with the static world whose normal is mostly vertical: part of the support polygon.
    pub floor: bool,
}

/// Everything the app needs to draw one moment of the simulation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimSnapshot {
    /// Simulation time, seconds.
    pub time: f64,
    /// Pose of every body, indexed by MuJoCo body id (the world body is 0).
    pub bodies: Vec<Pose>,
    /// Set when the run stopped because the simulation became unstable.
    pub error: Option<String>,
    /// Inspector data of every body, indexed like `bodies`.
    pub body_info: Vec<BodyView>,
    /// Every hinge.
    pub joints: Vec<JointView>,
    /// Every contact.
    pub contacts: Vec<ContactView>,
    /// Centre of mass of the whole robot (MuJoCo `subtree_com` of its root body), world frame.
    pub com: [f64; 3],
    /// Mass of the whole robot, kg.
    pub robot_mass: f64,
    /// The robot's root body id.
    pub root_body: usize,
    /// Distance and colour sensors as the sensors overlay draws them.
    pub sensors: Vec<SensorVisual>,
    /// Device names, indexed by the `device` of a [`SensorVisual`].
    pub device_names: Arc<Vec<String>>,
    /// Names of the built-in plot channels, in the order of `channel_values`.
    pub channel_names: Arc<Vec<String>>,
    /// Values of the built-in plot channels now.
    pub channel_values: Vec<f64>,
    /// A hash of every joint position (see [`joint_hash`]).
    pub hash: u64,
    /// True while the simulation is paused.
    pub paused: bool,
    /// The speed factor.
    pub speed: f64,
    /// While replaying to rewind: the time being replayed to.
    pub replay_target: Option<f64>,
}

/// A hash of all joint positions (`qpos`) of a run, to compare two runs exactly (FNV-1a over the
/// bit patterns).
pub fn joint_hash(physics: &Physics) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for q in physics.qpos() {
        for b in q.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Commands from the app to the sim thread.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Stop stepping.
    Pause,
    /// Continue.
    Resume,
    /// While paused, run this many 1 ms ticks (1 or 10 from the keyboard).
    Step {
        /// Milliseconds of simulated time.
        ms: u32,
    },
    /// Back to the start (the controller restarts, voltages stay).
    Reset,
    /// Rewind (or fast-forward) to this simulated time by deterministic replay from 0, with
    /// pacing off, then pause there. Drag forces and the controller are replayed.
    Rewind(f64),
    /// Pace against wall-clock time multiplied by this factor (0.1 to 4).
    SetSpeed(f64),
    /// Give a motor a command (power, speed, position, brake, coast).
    SetMotor {
        /// Motor (device) name.
        motor: String,
        /// The command.
        command: MotorCommand,
    },
    /// Apply this voltage to a motor from now on.
    SetVolts {
        /// Motor (device) name.
        motor: String,
        /// Applied voltage.
        volts: f64,
    },
    /// Which body is under this ray? The answer is a [`SimEvent::Picked`] with the same `id`.
    Pick {
        /// Echoed in the answer.
        id: u64,
        /// Ray origin, world frame.
        origin: [f64; 3],
        /// Ray direction (need not be unit length).
        dir: [f64; 3],
    },
    /// Apply a drag force to a body at a world point until replaced or cleared (the mouse spring).
    ApplyDragForce {
        /// MuJoCo body id.
        body: usize,
        /// World point of application.
        point: [f64; 3],
        /// Force, newtons.
        force: [f64; 3],
    },
    /// The mouse spring: pull a point of a body (given in the body's frame) towards a world target
    /// with a damped spring, evaluated every tick until replaced or cleared.
    DragTo {
        /// MuJoCo body id.
        body: usize,
        /// Grab point in the body's frame.
        local: [f64; 3],
        /// World point the cursor is at.
        target: [f64; 3],
    },
    /// The mouse drag ended: remove the drag force.
    ClearDrag,
    /// End the thread.
    Stop,
}

/// A push on the robot at a simulated time: an impulse delivered over one tick (a mission event).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Push {
    /// Simulated seconds.
    pub t: f64,
    /// Impulse, N s, world frame.
    pub impulse_ns: [f64; 3],
}

/// What a mission adds to the simulation besides the world geometry: the printed mat the colour
/// sensor reads and timed pushes.
#[derive(Clone, Debug, Default)]
pub struct Scenario {
    /// The floor mat (the floor geom named `floor` carries it).
    pub floor: Option<axl_hw::MatTexture>,
    /// Pushes on the robot's root body.
    pub pushes: Vec<Push>,
    /// The mission ends at this simulated time: `axl_step` then returns `AXL_MISSION_OVER`.
    pub limit_s: Option<f64>,
}

/// The stepping core: physics plus the hardware models. Shared by the thread and the headless run.
pub struct SimCore {
    physics: Physics,
    hw: Hardware,
    n_bodies: usize,
    last_publish: f64,
    volts: Vec<(String, f64)>,
    root: BodyId,
    n_joints: usize,
    contacts: Vec<ContactInfo>,
    device_names: Arc<Vec<String>>,
    channel_names: Arc<Vec<String>>,
    pushes: Vec<Push>,
    limit_s: Option<f64>,
}

impl SimCore {
    /// Load a compiled robot. `volts` is `(motor name, volts)`.
    pub fn new(robot: &Compiled, volts: &[(String, f64)]) -> Result<Self, SimError> {
        Self::with_options(robot, volts, HwOptions::default())
    }

    /// Load a compiled robot with explicit hardware options (seed, noise).
    pub fn with_options(
        robot: &Compiled,
        volts: &[(String, f64)],
        opts: HwOptions,
    ) -> Result<Self, SimError> {
        let physics = Physics::from_mjcf(&robot.mjcf)?;
        let hw = Hardware::new(&robot.devices, &HwParams::builtin(), opts, &physics);
        let root = BodyId(robot.bodies.bodies.first().map_or(1, |b| b.id));
        let device_names = Arc::new(hw.devices().iter().map(|d| d.name.clone()).collect());
        let mut names = hw.channel_names();
        names.extend(
            ["true x (m)", "true y (m)", "true yaw (rad)"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        let mut core = Self {
            n_bodies: physics.body_count(),
            n_joints: physics.joint_count(),
            physics,
            hw,
            last_publish: f64::NEG_INFINITY,
            volts: volts.to_vec(),
            root,
            contacts: Vec::new(),
            device_names,
            channel_names: Arc::new(names),
            pushes: Vec::new(),
            limit_s: None,
        };
        for (m, v) in volts {
            core.set_volts(m, *v)?;
        }
        Ok(core)
    }

    /// Add a mission's mat and pushes. The mat stays through resets.
    pub fn set_scenario(&mut self, scenario: &Scenario) {
        let floor = match (&scenario.floor, self.physics.geom_id("floor")) {
            (Some(texture), Ok(geom)) => Some(axl_hw::FloorMat {
                geom,
                texture: texture.clone(),
            }),
            _ => None,
        };
        self.hw.set_floor_mat(floor);
        self.pushes = scenario.pushes.clone();
        self.limit_s = scenario.limit_s;
    }

    /// The time at which the mission ends, if it has a limit.
    pub fn limit_s(&self) -> Option<f64> {
        self.limit_s
    }

    /// Give a motor a command.
    pub fn command(&mut self, motor: &str, command: MotorCommand) -> Result<(), SimError> {
        self.hw
            .motor_mut(motor)
            .ok_or_else(|| SimError::NoMotor(motor.into()))?
            .command(command);
        Ok(())
    }

    /// Set one motor's voltage.
    pub fn set_volts(&mut self, motor: &str, volts: f64) -> Result<(), SimError> {
        self.command(motor, MotorCommand::Volts(volts))
    }

    /// Simulation time.
    pub fn time(&self) -> f64 {
        self.physics.time()
    }

    /// The physics, for readings.
    pub fn physics(&self) -> &Physics {
        &self.physics
    }

    /// The physics, mutable (rays, applied forces).
    pub fn physics_mut(&mut self) -> &mut Physics {
        &mut self.physics
    }

    /// The hardware models, for readings.
    pub fn hardware(&self) -> &Hardware {
        &self.hw
    }

    /// The hardware models, mutable.
    pub fn hardware_mut(&mut self) -> &mut Hardware {
        &mut self.hw
    }

    /// The robot's root body.
    pub fn root_body(&self) -> BodyId {
        self.root
    }

    /// Back to the start. The voltages given at the start are applied again.
    pub fn reset(&mut self) {
        self.physics.reset();
        self.hw.reset(&self.physics);
        for (m, v) in self.volts.clone() {
            let _ = self.set_volts(&m, v);
        }
        self.last_publish = f64::NEG_INFINITY;
    }

    /// One 1 ms tick: hardware before, MuJoCo step, hardware after. Returns false when the
    /// simulation became unstable.
    pub fn tick(&mut self) -> bool {
        if !self.pushes.is_empty() {
            self.apply_pushes();
        }
        self.hw.pre_step(&mut self.physics);
        self.physics.step();
        self.hw.post_step(&mut self.physics);
        !self.physics.warnings().is_unstable()
    }

    /// A push is a force of `impulse / 1 ms` on the root body's centre of mass during one tick.
    fn apply_pushes(&mut self) {
        let ms = (self.physics.time() * 1000.0).round() as u64;
        for p in &self.pushes {
            let start = (p.t * 1000.0).round() as u64;
            if ms == start {
                let com = self.physics.body_com(self.root);
                let f = p.impulse_ns.map(|i| i / self.physics.timestep());
                self.physics.apply_force(self.root, com, f);
            } else if ms == start + 1 {
                self.physics.clear_applied_forces();
            }
        }
    }

    /// Write the current state into `out` in place (no allocation after the first call).
    /// Velocities, contacts and sensors describe the start of the step until `forward()` runs, so
    /// it runs first.
    pub fn fill(&mut self, out: &mut SimSnapshot) {
        self.physics.forward();
        out.time = self.physics.time();
        out.bodies.clear();
        out.bodies
            .extend((0..self.n_bodies).map(|i| self.physics.body_pose(BodyId(i))));
        out.body_info.clear();
        out.body_info.extend((0..self.n_bodies).map(|i| {
            let b = BodyId(i);
            BodyView {
                velocity: self.physics.body_linear_velocity(b),
                angular_velocity: self.physics.body_angular_velocity(b),
                com: self.physics.body_com(b),
                mass: self.physics.body_mass(b),
                inertia: self.physics.body_inertia(b),
            }
        }));
        out.joints.clear();
        for j in 0..self.n_joints {
            let id = JointId(j);
            if self.physics.joint_is_free(id) {
                continue;
            }
            let (anchor, axis) = self.physics.joint_anchor_axis(id);
            out.joints.push(JointView {
                joint: j,
                body: self.physics.joint_body(id).map_or(0, |b| b.0),
                anchor,
                axis,
                angle: self.physics.joint_pos(id),
                speed: self.physics.joint_vel(id),
                torque: self.physics.joint_torque(id),
            });
        }
        self.physics.contacts_into(&mut self.contacts);
        out.contacts.clear();
        for c in &self.contacts {
            let is_world = |g: GeomId| self.physics.geom_body(g) == Some(BodyId(0));
            out.contacts.push(ContactView {
                pos: c.pos,
                normal: c.normal,
                normal_force: c.normal_force,
                friction: c.friction_force,
                mu: c.friction_coef,
                floor: (is_world(c.geom1) || is_world(c.geom2)) && c.normal[2].abs() > 0.7,
            });
        }
        out.com = self.physics.subtree_com(self.root);
        out.robot_mass = self.physics.subtree_mass(self.root);
        out.root_body = self.root.0;
        self.hw.sensor_visuals(&mut self.physics, &mut out.sensors);
        out.device_names = self.device_names.clone();
        out.channel_names = self.channel_names.clone();
        self.hw.channel_values(&mut out.channel_values);
        let root = self.physics.body_pose(self.root);
        let q = root.quat;
        let yaw =
            (2.0 * (q[0] * q[3] + q[1] * q[2])).atan2(1.0 - 2.0 * (q[2] * q[2] + q[3] * q[3]));
        out.channel_values.extend([root.pos[0], root.pos[1], yaw]);
        out.hash = joint_hash(&self.physics);
    }

    /// True when a snapshot is due (every `interval` simulated seconds).
    pub(crate) fn due(&mut self, interval: f64) -> bool {
        let t = self.physics.time();
        if t - self.last_publish >= interval - 1e-9 {
            self.last_publish = t;
            true
        } else {
            false
        }
    }
}

/// Run a robot with fixed voltages for `seconds` of simulated time, as fast as possible (no
/// pacing), and return the final snapshot. Used by tests and headless runs.
pub fn run_headless(
    robot: &Compiled,
    volts: &[(String, f64)],
    seconds: f64,
) -> Result<SimSnapshot, SimError> {
    let mut core = SimCore::new(robot, volts)?;
    let mut snap = SimSnapshot::default();
    while core.time() < seconds - 1e-9 {
        if !core.tick() {
            snap.error = Some(UNSTABLE_MESSAGE.into());
            break;
        }
    }
    let err = snap.error.take();
    core.fill(&mut snap);
    snap.error = err;
    Ok(snap)
}

/// A running sim thread.
pub struct Sandbox {
    commands: Sender<Command>,
    events: Receiver<SimEvent>,
    output: Output<SimSnapshot>,
    thread: Option<JoinHandle<()>>,
}

impl Sandbox {
    /// Start the sim thread. The first snapshot is available once the thread has loaded the model.
    pub fn spawn(robot: &Compiled, volts: &[(String, f64)]) -> Result<Self, SimError> {
        Self::start(
            robot,
            volts,
            HwOptions::default(),
            None,
            true,
            &Scenario::default(),
        )
    }

    /// Start the sim thread with a learner program. `paced` runs against the wall clock; unpaced
    /// runs as fast as possible (tests). Events stream through [`Sandbox::events`].
    pub fn spawn_program(
        robot: &Compiled,
        volts: &[(String, f64)],
        opts: HwOptions,
        program: ProgramConfig,
        paced: bool,
    ) -> Result<Self, SimError> {
        Self::start(
            robot,
            volts,
            opts,
            Some(program),
            paced,
            &Scenario::default(),
        )
    }

    /// Like [`Sandbox::spawn_program`], inside a mission's scenario (mat and pushes). `program`
    /// may be `None`: the robot stands still and the world runs (the Simulate screen's sandbox).
    pub fn spawn_scenario(
        robot: &Compiled,
        volts: &[(String, f64)],
        opts: HwOptions,
        program: Option<ProgramConfig>,
        paced: bool,
        scenario: &Scenario,
    ) -> Result<Self, SimError> {
        Self::start(robot, volts, opts, program, paced, scenario)
    }

    fn start(
        robot: &Compiled,
        volts: &[(String, f64)],
        opts: HwOptions,
        program: Option<ProgramConfig>,
        paced: bool,
        scenario: &Scenario,
    ) -> Result<Self, SimError> {
        // Load on this thread first so the caller gets load errors, then let the thread build its
        // own copy (MuJoCo handles stay on the sim thread).
        SimCore::with_options(robot, volts, opts)?;
        let robot = robot.clone();
        let volts = volts.to_vec();
        let scenario = scenario.clone();
        let (tx, rx) = unbounded();
        let (etx, events) = unbounded();
        let (input, output) = triple_buffer(&SimSnapshot::default());
        let thread = std::thread::Builder::new()
            .name("axl-sim".into())
            .spawn(move || {
                if let Ok(mut core) = SimCore::with_options(&robot, &volts, opts) {
                    core.set_scenario(&scenario);
                    program::sim_thread(core, program, rx, input, etx, paced);
                }
            })
            .ok();
        Ok(Self {
            commands: tx,
            events,
            output,
            thread,
        })
    }

    /// What the simulation has reported since the last call (logs, plots, drawings, picks,
    /// checkpoints, restarts, the end of the program).
    pub fn events(&self) -> &Receiver<SimEvent> {
        &self.events
    }

    /// Send a command to the sim thread.
    pub fn send(&self, c: Command) {
        let _ = self.commands.send(c);
    }

    /// The newest published snapshot.
    pub fn latest(&mut self) -> &SimSnapshot {
        self.output.read()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
