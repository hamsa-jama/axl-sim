//! A learner program on the sim thread: [`SimRobot`] implements the host's [`RobotApi`] on top of
//! the stepping core, so `axl_step` runs 1 ms ticks paced against the wall clock. When `main`
//! returns, every motor brakes and the simulation carries on by itself.
//!
//! The same loop serves the sandbox (no program) and handles the time controls: pause, single
//! steps, speed, reset, and rewind by deterministic replay from 0 (the controller restarts, runs
//! flat out to the target time and the simulation pauses there). Drag forces are logged by
//! simulated time so a replay applies them again.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use axl_host::{
    Controller, DeviceKind, HostConfig, LoadError, LogStream, MotorCommand, MotorReading, RobotApi,
    RunEnd, StepStatus,
};
use axl_hw::{ColorName, Slot};
use axl_mujoco::{BodyId, GroupMask, Pose};
use crossbeam_channel::{Receiver, Sender};
use triple_buffer::Input;

use crate::mission::Recorder;
use crate::{Command, SimCore, SimSnapshot, UNSTABLE_MESSAGE, joint_hash};

/// Most plot channels one program may use (SPEC).
pub const MAX_PLOT_CHANNELS: usize = 16;

/// Seconds of simulated time between snapshots while a replay runs flat out.
const REPLAY_SNAPSHOT_INTERVAL_S: f64 = 0.05;

/// Things the simulation reports that the app shows.
#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    /// The program started.
    Started,
    /// The simulation went back to time 0 (a reset or a rewind). Plots, trails and drawings from
    /// the old run are stale: a rewind replays them.
    Restarted {
        /// The time being replayed to, for a rewind.
        replay_to: Option<f64>,
    },
    /// A line from stdout or stderr.
    Log {
        /// Which stream.
        stream: LogStream,
        /// The text.
        line: String,
    },
    /// A point on a plot channel.
    Plot {
        /// Channel name.
        channel: String,
        /// Simulated seconds.
        time: f64,
        /// Value.
        value: f64,
    },
    /// The robot's believed pose.
    Pose {
        /// Simulated seconds.
        time: f64,
        /// Metres.
        x: f64,
        /// Metres.
        y: f64,
        /// Radians.
        yaw: f64,
    },
    /// A line drawn in the 3D view.
    Line {
        /// Start.
        from: [f64; 3],
        /// End.
        to: [f64; 3],
        /// 0xRRGGBBAA.
        rgba: u32,
    },
    /// The drawn lines are cleared.
    ClearLines,
    /// A value the mission asked the robot to measure.
    Report {
        /// Key.
        key: String,
        /// Value.
        value: f64,
    },
    /// The answer to a [`Command::Pick`]: the nearest robot or object body under the ray.
    Picked {
        /// The id of the command.
        id: u64,
        /// What was hit.
        hit: Option<PickHit>,
    },
    /// The state at a whole simulated second, for the scrub bar's preview.
    Checkpoint {
        /// Simulated seconds (a whole number).
        time: f64,
        /// Joint-position hash at that time (see [`crate::joint_hash`]).
        hash: u64,
        /// Pose of every body.
        bodies: Vec<Pose>,
    },
    /// The program ended (or could not load). The simulation carries on.
    Ended(Result<RunEnd, String>),
}

/// What a pick ray hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickHit {
    /// MuJoCo body id.
    pub body: usize,
    /// World point of the hit.
    pub point: [f64; 3],
    /// Distance along the ray, metres.
    pub distance: f64,
}

/// One drag command with the simulated millisecond it was given at.
#[derive(Clone, Copy, Debug)]
struct DragEvent {
    ms: u64,
    action: Option<DragAction>,
}

/// What a drag command asks for (`None` in a [`DragEvent`] clears the drag).
#[derive(Clone, Copy, Debug)]
enum DragAction {
    /// A constant force at a world point.
    Force(usize, [f64; 3], [f64; 3]),
    /// A spring from a point of the body (in its frame) to a world target, evaluated every tick.
    Spring(usize, [f64; 3], [f64; 3]),
}

/// How stiff the mouse spring is, as a rate: it pulls with `mass * OMEGA^2` per metre. A feel
/// parameter of the interface, not a physical constant.
const SPRING_OMEGA: f64 = 12.0;
/// Damping ratio of the mouse spring.
const SPRING_ZETA: f64 = 0.7;
/// The spring never pulls with more than this many times the robot's weight.
const SPRING_MAX_WEIGHTS: f64 = 6.0;

/// Rotate `v` by the unit quaternion `q` = `[w, x, y, z]`.
fn quat_rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let [w, x, y, z] = q;
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

struct Inner {
    core: SimCore,
    out: Input<SimSnapshot>,
    commands: Receiver<Command>,
    events: Sender<SimEvent>,
    paced: bool,
    paused: bool,
    speed: f64,
    failed: bool,
    stop: bool,
    restart: bool,
    /// Set with `restart` by a rewind: replay to this time, then pause.
    pending_replay: Option<f64>,
    /// While replaying: the time to reach.
    replay_to: Option<f64>,
    /// Ticks still allowed while paused (single steps).
    step_budget: u32,
    stepping: bool,
    origin: Instant,
    origin_sim: f64,
    /// What the newest snapshot said about (paused, speed), so a change publishes again.
    flags: (bool, f64),
    channels: Vec<String>,
    drag_log: Vec<DragEvent>,
    drag_next: usize,
    spring: Option<(usize, [f64; 3], [f64; 3])>,
    /// A graded headless run: no snapshots, a recorder samples the run instead.
    recorder: Option<Recorder>,
}

impl Inner {
    fn sim_ms(&self) -> u64 {
        (self.core.time() * 1000.0).round() as u64
    }

    fn publish(&mut self, error: Option<&str>) {
        let buf = self.out.input_buffer_mut();
        self.core.fill(buf);
        buf.error = error.map(str::to_owned);
        buf.paused = self.paused;
        buf.speed = self.speed;
        buf.replay_target = self.replay_to;
        self.flags = (self.paused, self.speed);
        self.out.publish();
    }

    fn rebase(&mut self) {
        self.origin = Instant::now();
        self.origin_sim = self.core.time();
    }

    fn checkpoint(&mut self) {
        let physics = self.core.physics();
        let bodies: Vec<Pose> = (0..physics.body_count())
            .map(|i| physics.body_pose(BodyId(i)))
            .collect();
        let hash = joint_hash(physics);
        let time = self.core.time().round();
        self.send(SimEvent::Checkpoint { time, hash, bodies });
    }

    fn pick(&mut self, id: u64, origin: [f64; 3], dir: [f64; 3]) {
        // Robot (group 1) and loose objects (group 2): the floor and walls are not pickable.
        self.core.physics_mut().forward();
        let hit = self
            .core
            .physics_mut()
            .ray(origin, dir, GroupMask(0b0000_0110))
            .and_then(|h| {
                let body = self.core.physics().geom_body(h.geom)?;
                Some(PickHit {
                    body: body.0,
                    point: h.point,
                    distance: h.distance,
                })
            });
        self.send(SimEvent::Picked { id, hit });
    }

    fn drag(&mut self, action: Option<DragAction>) {
        if self.replay_to.is_some() {
            return; // the logged drags are being replayed
        }
        let ms = self.sim_ms();
        self.drag_log.push(DragEvent { ms, action });
        self.drag_next = self.drag_log.len();
        self.apply_drag(action);
    }

    fn apply_drag(&mut self, action: Option<DragAction>) {
        self.spring = None;
        match action {
            Some(DragAction::Force(body, point, f)) => {
                self.core.physics_mut().apply_force(BodyId(body), point, f);
            }
            Some(DragAction::Spring(body, local, target)) => {
                self.spring = Some((body, local, target));
            }
            None => self.core.physics_mut().clear_applied_forces(),
        }
    }

    /// The mouse spring: pull the grab point towards the target with a damped spring, once per
    /// tick, so it stays stable however slowly the window draws.
    fn apply_spring(&mut self) {
        let Some((body, local, target)) = self.spring else {
            return;
        };
        let root = self.core.root_body();
        let physics = self.core.physics_mut();
        let b = BodyId(body);
        // Positions and velocities are stale after a step until `forward`: always refresh them, or
        // the force would depend on whether a snapshot happened to be published in between (and a
        // replay would differ).
        physics.forward();
        let pose = physics.body_pose(b);
        let r = quat_rotate(pose.quat, local);
        let grab = [pose.pos[0] + r[0], pose.pos[1] + r[1], pose.pos[2] + r[2]];
        let com = physics.body_com(b);
        let vel = physics.body_linear_velocity(b);
        let w = physics.body_angular_velocity(b);
        let arm = [grab[0] - com[0], grab[1] - com[1], grab[2] - com[2]];
        let v = [
            vel[0] + w[1] * arm[2] - w[2] * arm[1],
            vel[1] + w[2] * arm[0] - w[0] * arm[2],
            vel[2] + w[0] * arm[1] - w[1] * arm[0],
        ];
        let m = physics.subtree_mass(root).max(1e-3);
        let k = m * SPRING_OMEGA * SPRING_OMEGA;
        let c = 2.0 * SPRING_ZETA * m * SPRING_OMEGA;
        let mut f = [0.0; 3];
        for i in 0..3 {
            f[i] = k * (target[i] - grab[i]) - c * v[i];
        }
        let norm = f.iter().map(|x| x * x).sum::<f64>().sqrt();
        let cap = SPRING_MAX_WEIGHTS * m * 9.81;
        if norm > cap {
            for x in &mut f {
                *x *= cap / norm;
            }
        }
        physics.apply_force(b, grab, f);
    }

    /// While replaying, apply the logged drag commands that are due.
    fn replay_drags(&mut self) {
        let ms = self.sim_ms();
        while let Some(e) = self.drag_log.get(self.drag_next).copied() {
            if e.ms > ms {
                break;
            }
            self.drag_next += 1;
            self.apply_drag(e.action);
        }
    }

    fn poll(&mut self) {
        while let Ok(c) = self.commands.try_recv() {
            match c {
                Command::Stop => self.stop = true,
                Command::Pause => self.paused = true,
                Command::Resume => {
                    self.paused = false;
                    self.step_budget = 0;
                    self.rebase();
                }
                Command::Step { ms } => {
                    self.paused = true;
                    self.step_budget = (self.step_budget + ms).min(1000);
                }
                Command::Reset => {
                    self.restart = true;
                    self.pending_replay = None;
                    self.drag_log.clear();
                    self.spring = None;
                }
                Command::Rewind(t) => {
                    self.restart = true;
                    self.pending_replay = Some(t.max(0.0));
                }
                Command::SetSpeed(s) => {
                    self.speed = s.clamp(0.1, 4.0);
                    self.rebase();
                }
                Command::SetVolts { motor, volts } => {
                    let _ = self.core.set_volts(&motor, volts);
                }
                Command::SetMotor { motor, command } => {
                    let _ = self.core.command(&motor, command);
                }
                Command::Pick { id, origin, dir } => self.pick(id, origin, dir),
                Command::ApplyDragForce { body, point, force } => {
                    self.drag(Some(DragAction::Force(body, point, force)));
                }
                Command::DragTo {
                    body,
                    local,
                    target,
                } => self.drag(Some(DragAction::Spring(body, local, target))),
                Command::ClearDrag => self.drag(None),
            }
        }
    }

    /// Wait until the next 1 ms tick is due. False when the program should end (stop or restart).
    fn wait_turn(&mut self) -> bool {
        loop {
            self.poll();
            if (self.paused, self.speed) != self.flags {
                self.publish(None);
            }
            if self.stop || self.restart {
                return false;
            }
            if let Some(target) = self.replay_to {
                if self.core.time() + 1e-9 >= target {
                    // Arrived: drops the log entries after this time (they are the old future) and
                    // waits here, paused.
                    let ms = self.sim_ms();
                    self.drag_log.retain(|e| e.ms <= ms);
                    self.drag_next = self.drag_log.len();
                    self.replay_to = None;
                    self.paused = true;
                    self.step_budget = 0;
                    self.publish(None);
                    continue;
                }
                return true; // flat out: no pacing while replaying
            }
            if self.paused {
                if self.step_budget > 0 {
                    self.step_budget -= 1;
                    self.stepping = true;
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            if !self.paced {
                return true;
            }
            let target = self.origin_sim + self.origin.elapsed().as_secs_f64() * self.speed;
            if self.core.time() >= target {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            if target - self.core.time() > 0.05 {
                self.rebase(); // never catch up more than 50 ms at once
            }
            return true;
        }
    }

    /// One paced tick. False when interrupted; `Err` when the simulation blew up.
    fn tick(&mut self) -> Result<bool, String> {
        if !self.wait_turn() {
            return Ok(false);
        }
        if let Some(limit) = self.core.limit_s()
            && self.core.time() >= limit - 1e-9
        {
            return Ok(false);
        }
        if self.replay_to.is_some() {
            self.replay_drags();
        }
        self.apply_spring();
        if !self.core.tick() {
            self.failed = true;
            self.replay_to = None;
            self.publish(Some(UNSTABLE_MESSAGE));
            return Err(UNSTABLE_MESSAGE.into());
        }
        if let Some(rec) = self.recorder.as_mut() {
            rec.after_tick(&mut self.core);
            return Ok(true);
        }
        if self.sim_ms().is_multiple_of(1000) {
            self.checkpoint();
        }
        let interval = if self.replay_to.is_some() {
            REPLAY_SNAPSHOT_INTERVAL_S
        } else {
            crate::SNAPSHOT_INTERVAL_S
        };
        let due = self.core.due(interval);
        if std::mem::take(&mut self.stepping) || due {
            self.publish(None);
        }
        Ok(true)
    }

    fn hw_slot(&self, dev: usize) -> Option<Slot> {
        self.core.hardware().devices().get(dev).map(|d| d.slot)
    }

    fn send(&self, e: SimEvent) {
        let _ = self.events.send(e);
    }
}

/// The robot a controller sees: a handle to the sim thread's state.
#[derive(Clone)]
pub struct SimRobot(Rc<RefCell<Inner>>);

impl RobotApi for SimRobot {
    fn step(&mut self, ms: u32) -> Result<StepStatus, String> {
        let mut s = self.0.borrow_mut();
        for _ in 0..ms {
            if s.failed {
                return Err(UNSTABLE_MESSAGE.into());
            }
            if !s.tick()? {
                return Ok(StepStatus::MissionOver);
            }
        }
        Ok(StepStatus::Running)
    }

    fn time(&self) -> f64 {
        self.0.borrow().core.time()
    }

    fn find_device(&self, name: &str) -> Option<usize> {
        self.0.borrow().core.hardware().find(name)
    }

    fn device_info(&self, dev: usize) -> Option<(String, DeviceKind)> {
        self.0
            .borrow()
            .core
            .hardware()
            .devices()
            .get(dev)
            .map(|d| (d.name.clone(), d.kind))
    }

    fn motor_command(&mut self, dev: usize, cmd: MotorCommand) {
        let mut s = self.0.borrow_mut();
        if let Some(Slot::Motor(i)) = s.hw_slot(dev)
            && let Some(m) = s.core.hardware_mut().motor_at_mut(i)
        {
            m.command(cmd);
        }
    }

    fn motor_reset_angle(&mut self, dev: usize) {
        let mut s = self.0.borrow_mut();
        if let Some(Slot::Motor(i)) = s.hw_slot(dev)
            && let Some(m) = s.core.hardware_mut().motor_at_mut(i)
        {
            m.reset_angle();
        }
    }

    fn motor_reading(&self, dev: usize, what: MotorReading) -> f64 {
        let s = self.0.borrow();
        let Some(Slot::Motor(i)) = s.hw_slot(dev) else {
            return 0.0;
        };
        let Some(m) = s.core.hardware().motor_at(i) else {
            return 0.0;
        };
        match what {
            MotorReading::Angle => m.angle(),
            MotorReading::Rate => m.rate(),
            MotorReading::Current => m.current(),
            MotorReading::Done => f64::from(u8::from(m.done())),
        }
    }

    fn distance(&self, dev: usize) -> f64 {
        let s = self.0.borrow();
        s.hw_slot(dev)
            .and_then(|slot| s.core.hardware().distance(slot))
            .unwrap_or(-1.0)
    }

    fn color(&self, dev: usize) -> ([i32; 3], i32, ColorName) {
        let s = self.0.borrow();
        let hw = s.core.hardware();
        match s.hw_slot(dev) {
            Some(Slot::Color(i)) => hw.color_at(i).map_or(([0; 3], 0, ColorName::None), |c| {
                let r = c.reading(hw.now_ms());
                (r.rgb, r.reflect.round() as i32, r.name)
            }),
            _ => ([0; 3], 0, ColorName::None),
        }
    }

    fn touch(&self, dev: usize) -> (bool, f64) {
        let s = self.0.borrow();
        let hw = s.core.hardware();
        match s.hw_slot(dev) {
            Some(Slot::Touch(i)) => hw.touch_at(i).map_or((false, 0.0), |t| {
                (t.pressed(hw.now_ms()), t.force_reading(hw.now_ms()))
            }),
            _ => (false, 0.0),
        }
    }

    fn imu_gyro(&self) -> [f64; 3] {
        let s = self.0.borrow();
        let hw = s.core.hardware();
        hw.imu().map_or([0.0; 3], |i| i.gyro(hw.now_ms()))
    }

    fn imu_accel(&self) -> [f64; 3] {
        let s = self.0.borrow();
        let hw = s.core.hardware();
        hw.imu().map_or([0.0; 3], |i| i.accel(hw.now_ms()))
    }

    fn battery_voltage(&self) -> f64 {
        self.0.borrow().core.hardware().battery_voltage()
    }

    fn plot(&mut self, channel: &str, value: f64) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        if let Some(r) = s.recorder.as_mut() {
            r.plot_used = true;
        }
        if !s.channels.iter().any(|c| c == channel) {
            if s.channels.len() >= MAX_PLOT_CHANNELS {
                return Err(format!(
                    "A program can plot up to {MAX_PLOT_CHANNELS} channels. \"{channel}\" would be one more."
                ));
            }
            s.channels.push(channel.to_owned());
        }
        let time = s.core.time();
        s.send(SimEvent::Plot {
            channel: channel.to_owned(),
            time,
            value,
        });
        Ok(())
    }

    fn mark_pose(&mut self, x: f64, y: f64, yaw: f64) {
        let mut s = self.0.borrow_mut();
        let t = s.core.time();
        if let Some(r) = s.recorder.as_mut() {
            r.believed.push((t, [x, y, yaw]));
        }
        s.send(SimEvent::Pose {
            time: s.core.time(),
            x,
            y,
            yaw,
        });
    }

    fn draw_line(&mut self, from: [f64; 3], to: [f64; 3], rgba: u32) {
        self.0.borrow().send(SimEvent::Line { from, to, rgba });
    }

    fn draw_clear(&mut self) {
        self.0.borrow().send(SimEvent::ClearLines);
    }

    fn report(&mut self, key: &str, value: f64) {
        let mut s = self.0.borrow_mut();
        let t = s.core.time();
        if let Some(r) = s.recorder.as_mut() {
            r.reports.push((t, key.to_owned(), value));
        }
        drop(s);
        self.0.borrow().send(SimEvent::Report {
            key: key.to_owned(),
            value,
        });
    }

    fn log(&mut self, stream: LogStream, line: &str) {
        self.0.borrow().send(SimEvent::Log {
            stream,
            line: line.to_owned(),
        });
    }

    fn program_ended(&mut self) {
        let mut s = self.0.borrow_mut();
        for m in s.core.hardware_mut().motors_mut() {
            m.command(MotorCommand::Brake);
        }
    }
}

/// Options of a program run on the sim thread.
#[derive(Clone, Debug)]
pub struct ProgramConfig {
    /// The program, `.wasm` bytes.
    pub wasm: Vec<u8>,
    /// Limits and disabled functions.
    pub host: HostConfig,
}

/// The sim thread body: run the program (if any), then keep the simulation going until stopped.
/// A `Reset` or `Rewind` restarts the simulation and the program.
pub(crate) fn sim_thread(
    mut core: SimCore,
    program: Option<ProgramConfig>,
    commands: Receiver<Command>,
    out: Input<SimSnapshot>,
    events: Sender<SimEvent>,
    paced: bool,
) {
    core.reset();
    let inner = Rc::new(RefCell::new(Inner {
        core,
        out,
        commands,
        events,
        paced,
        paused: false,
        speed: 1.0,
        failed: false,
        stop: false,
        restart: false,
        pending_replay: None,
        replay_to: None,
        step_budget: 0,
        stepping: false,
        origin: Instant::now(),
        origin_sim: 0.0,
        flags: (false, 1.0),
        channels: Vec::new(),
        drag_log: Vec::new(),
        drag_next: 0,
        spring: None,
        recorder: None,
    }));
    {
        let mut s = inner.borrow_mut();
        s.publish(None);
        s.checkpoint();
    }
    loop {
        {
            let mut s = inner.borrow_mut();
            if s.restart {
                s.core.reset();
                s.failed = false;
                s.restart = false;
                s.channels.clear();
                s.replay_to = s.pending_replay.take();
                s.drag_next = 0;
                s.spring = None;
                s.step_budget = 0;
                s.rebase();
                s.publish(None);
                s.checkpoint();
                let replay_to = s.replay_to;
                s.send(SimEvent::Restarted { replay_to });
            }
        }
        if let Some(program) = &program {
            inner.borrow().send(SimEvent::Started);
            match Controller::new(&program.wasm, SimRobot(inner.clone()), &program.host) {
                Ok(mut c) => {
                    let end = c.run();
                    inner.borrow().send(SimEvent::Ended(Ok(end)));
                }
                Err(e) => {
                    inner.borrow().send(SimEvent::Ended(Err(load_message(&e))));
                    SimRobot(inner.clone()).program_ended();
                }
            }
        }
        if inner.borrow().stop {
            return;
        }
        // The simulation carries on without a program until stopped, reset or rewound.
        loop {
            let mut s = inner.borrow_mut();
            if s.stop {
                return;
            }
            if s.restart {
                break;
            }
            if s.failed {
                s.poll();
                drop(s);
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            match s.tick() {
                Err(_) => continue,
                Ok(false) if !s.restart && !s.stop => {
                    // The mission's time limit: stay here until reset or stopped.
                    drop(s);
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(_) => {}
            }
        }
    }
}

/// What an unpaced graded run leaves behind.
pub(crate) struct Headless {
    pub recorder: Recorder,
    pub end: Option<Result<RunEnd, String>>,
    pub unstable: bool,
    pub logs: Vec<String>,
}

/// Run the program (if any) flat out, then the rest of the mission up to its time limit,
/// recording a trace.
pub(crate) fn run_unpaced(
    mut core: SimCore,
    program: Option<&ProgramConfig>,
    recorder: Recorder,
) -> Headless {
    core.reset();
    let (etx, erx) = crossbeam_channel::unbounded();
    let (ctx, crx) = crossbeam_channel::unbounded::<Command>();
    let (input, _output) = triple_buffer::triple_buffer(&SimSnapshot::default());
    let inner = Rc::new(RefCell::new(Inner {
        core,
        out: input,
        commands: crx,
        events: etx,
        paced: false,
        paused: false,
        speed: 1.0,
        failed: false,
        stop: false,
        restart: false,
        pending_replay: None,
        replay_to: None,
        step_budget: 0,
        stepping: false,
        origin: Instant::now(),
        origin_sim: 0.0,
        flags: (false, 1.0),
        channels: Vec::new(),
        drag_log: Vec::new(),
        drag_next: 0,
        spring: None,
        recorder: Some(recorder),
    }));
    {
        let mut s = inner.borrow_mut();
        if let Some(mut r) = s.recorder.take() {
            r.start(&mut s.core);
            s.recorder = Some(r);
        }
    }
    let mut end = None;
    if let Some(program) = program {
        match Controller::new(&program.wasm, SimRobot(inner.clone()), &program.host) {
            Ok(mut c) => end = Some(Ok(c.run())),
            Err(e) => {
                end = Some(Err(load_message(&e)));
                SimRobot(inner.clone()).program_ended();
            }
        }
    }
    let mut unstable = inner.borrow().failed;
    let stopped_with_error = matches!(end, Some(Ok(RunEnd::Stopped(_))) | Some(Err(_)));
    if !unstable && !stopped_with_error {
        loop {
            match inner.borrow_mut().tick() {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    unstable = true;
                    break;
                }
            }
        }
    }
    drop(ctx);
    let mut logs = Vec::new();
    while let Ok(e) = erx.try_recv() {
        if let SimEvent::Log { line, .. } = e {
            logs.push(line);
        }
    }
    let recorder = inner.borrow_mut().recorder.take().unwrap_or_default();
    Headless {
        recorder,
        end,
        unstable,
        logs,
    }
}

fn load_message(e: &LoadError) -> String {
    e.to_string()
}
