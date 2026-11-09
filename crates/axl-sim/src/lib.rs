//! axl-sim: the sim thread. It owns MuJoCo and steps it in lockstep at 1 ms, paced against the
//! wall clock, and publishes a [`SimSnapshot`] of every body pose through a triple buffer.
//!
//! Milestone M5 has the sandbox drive: a compiled robot with fixed motor voltages. The hardware
//! models (M6) and the controller (M7) join the step loop later.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axl_compile::Compiled;
use axl_mujoco::{ActuatorId, BodyId, Physics, PhysicsError, Pose};
use crossbeam_channel::{Receiver, Sender, unbounded};
use triple_buffer::{Input, Output, triple_buffer};

/// Simulated seconds between published snapshots (250 Hz cap).
pub const SNAPSHOT_INTERVAL_S: f64 = 0.004;
/// The message shown when the simulation blows up (SPEC, "The step loop").
pub const UNSTABLE_MESSAGE: &str =
    "The simulation became unstable. This usually means a part is very light or a joint is overloaded.";

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

/// Everything the app needs to draw one moment of the simulation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimSnapshot {
    /// Simulation time, seconds.
    pub time: f64,
    /// Pose of every body, indexed by MuJoCo body id (the world body is 0).
    pub bodies: Vec<Pose>,
    /// Set when the run stopped because the simulation became unstable.
    pub error: Option<String>,
}

/// Commands from the app to the sim thread.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Stop stepping.
    Pause,
    /// Continue.
    Resume,
    /// Back to the start (voltages stay).
    Reset,
    /// Pace against wall-clock time multiplied by this factor.
    SetSpeed(f64),
    /// Apply this voltage to a motor from now on.
    SetVolts {
        /// Motor (device) name.
        motor: String,
        /// Applied voltage.
        volts: f64,
    },
    /// End the thread.
    Stop,
}

/// The stepping core: physics plus the voltages to apply. Shared by the thread and the headless run.
pub struct SimCore {
    physics: Physics,
    ctrl: Vec<(ActuatorId, f64)>,
    names: Vec<(String, usize)>,
    n_bodies: usize,
    last_publish: f64,
}

impl SimCore {
    /// Load a compiled robot. `volts` is `(motor name, volts)`.
    pub fn new(robot: &Compiled, volts: &[(String, f64)]) -> Result<Self, SimError> {
        let physics = Physics::from_mjcf(&robot.mjcf)?;
        let mut names = Vec::new();
        for d in &robot.devices.devices {
            if let Some(a) = &d.actuator {
                names.push((d.name.clone(), a.id));
            }
        }
        let mut core = Self {
            n_bodies: physics.body_count(),
            physics,
            ctrl: names.iter().map(|(_, id)| (ActuatorId(*id), 0.0)).collect(),
            names,
            last_publish: f64::NEG_INFINITY,
        };
        for (m, v) in volts {
            core.set_volts(m, *v)?;
        }
        Ok(core)
    }

    /// Set one motor's voltage.
    pub fn set_volts(&mut self, motor: &str, volts: f64) -> Result<(), SimError> {
        let i = self
            .names
            .iter()
            .position(|(n, _)| n == motor)
            .ok_or_else(|| SimError::NoMotor(motor.into()))?;
        self.ctrl[i].1 = volts;
        Ok(())
    }

    /// Simulation time.
    pub fn time(&self) -> f64 {
        self.physics.time()
    }

    /// The physics, for readings.
    pub fn physics(&self) -> &Physics {
        &self.physics
    }

    /// Back to the start.
    pub fn reset(&mut self) {
        self.physics.reset();
        self.last_publish = f64::NEG_INFINITY;
    }

    /// One 1 ms tick. Returns false when the simulation became unstable.
    pub fn tick(&mut self) -> bool {
        for (a, v) in &self.ctrl {
            self.physics.set_ctrl(*a, *v);
        }
        self.physics.step();
        !self.physics.warnings().is_unstable()
    }

    /// Write the current poses into `out` in place (no allocation after the first call).
    pub fn fill(&self, out: &mut SimSnapshot) {
        out.time = self.physics.time();
        out.bodies.clear();
        out.bodies
            .extend((0..self.n_bodies).map(|i| self.physics.body_pose(BodyId(i))));
    }

    fn due(&mut self) -> bool {
        let t = self.physics.time();
        if t - self.last_publish >= SNAPSHOT_INTERVAL_S - 1e-9 {
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
    output: Output<SimSnapshot>,
    thread: Option<JoinHandle<()>>,
}

impl Sandbox {
    /// Start the sim thread. The first snapshot is available once the thread has loaded the model.
    pub fn spawn(robot: &Compiled, volts: &[(String, f64)]) -> Result<Self, SimError> {
        // Load on this thread first so the caller gets load errors, then let the thread build
        // its own copy (MuJoCo handles stay on the sim thread).
        SimCore::new(robot, volts)?;
        let (tx, rx) = unbounded();
        let (input, output) = triple_buffer(&SimSnapshot::default());
        let robot = robot.clone();
        let volts = volts.to_vec();
        let thread = std::thread::Builder::new()
            .name("axl-sim".into())
            .spawn(move || sim_thread(&robot, &volts, rx, input))
            .ok();
        Ok(Self {
            commands: tx,
            output,
            thread,
        })
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

fn sim_thread(
    robot: &Compiled,
    volts: &[(String, f64)],
    commands: Receiver<Command>,
    mut out: Input<SimSnapshot>,
) {
    let Ok(mut core) = SimCore::new(robot, volts) else {
        return;
    };
    let publish = |core: &SimCore, out: &mut Input<SimSnapshot>, error: Option<&str>| {
        let buf = out.input_buffer_mut();
        core.fill(buf);
        buf.error = error.map(str::to_owned);
        out.publish();
    };
    core.physics.forward();
    publish(&core, &mut out, None);
    let (mut paused, mut speed, mut failed) = (false, 1.0_f64, false);
    // Wall-clock origin and the simulated time it corresponds to.
    let mut origin = Instant::now();
    let mut origin_sim = 0.0;
    loop {
        while let Ok(c) = commands.try_recv() {
            match c {
                Command::Stop => return,
                Command::Pause => paused = true,
                Command::Resume => {
                    paused = false;
                    origin = Instant::now();
                    origin_sim = core.time();
                }
                Command::Reset => {
                    core.reset();
                    core.physics.forward();
                    failed = false;
                    origin = Instant::now();
                    origin_sim = 0.0;
                    publish(&core, &mut out, None);
                }
                Command::SetSpeed(s) => {
                    speed = s.clamp(0.1, 4.0);
                    origin = Instant::now();
                    origin_sim = core.time();
                }
                Command::SetVolts { motor, volts } => {
                    let _ = core.set_volts(&motor, volts);
                }
            }
        }
        if paused || failed {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        // Pace: step until simulated time reaches wall time x speed, but never more than 50 ms
        // of catching up in one go.
        let target = origin_sim + origin.elapsed().as_secs_f64() * speed;
        let limit = core.time() + 0.05;
        let mut steps = 0;
        while core.time() < target.min(limit) {
            if !core.tick() {
                failed = true;
                publish(&core, &mut out, Some(UNSTABLE_MESSAGE));
                break;
            }
            steps += 1;
            if core.due() {
                publish(&core, &mut out, None);
            }
        }
        if steps == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
