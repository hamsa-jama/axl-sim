//! A learner program on the sim thread: [`SimRobot`] implements the host's [`RobotApi`] on top of
//! the stepping core, so `axl_step` runs 1 ms ticks paced against the wall clock. When `main`
//! returns, every motor brakes and the simulation carries on by itself.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use axl_host::{
    Controller, DeviceKind, HostConfig, LoadError, LogStream, MotorCommand, MotorReading, RobotApi,
    RunEnd, StepStatus,
};
use axl_hw::{ColorName, Slot};
use crossbeam_channel::{Receiver, Sender};
use triple_buffer::Input;

use crate::{Command, SimCore, SimSnapshot, UNSTABLE_MESSAGE};

/// Most plot channels one program may use (SPEC).
pub const MAX_PLOT_CHANNELS: usize = 16;

/// Things the program does that the app shows.
#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    /// The program started.
    Started,
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
    /// The program ended (or could not load). The simulation carries on.
    Ended(Result<RunEnd, String>),
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
    origin: Instant,
    origin_sim: f64,
    channels: Vec<String>,
}

impl Inner {
    fn publish(&mut self, error: Option<&str>) {
        let buf = self.out.input_buffer_mut();
        self.core.fill(buf);
        buf.error = error.map(str::to_owned);
        self.out.publish();
    }

    fn rebase(&mut self) {
        self.origin = Instant::now();
        self.origin_sim = self.core.time();
    }

    fn poll(&mut self) {
        while let Ok(c) = self.commands.try_recv() {
            match c {
                Command::Stop => self.stop = true,
                Command::Pause => self.paused = true,
                Command::Resume => {
                    self.paused = false;
                    self.rebase();
                }
                Command::Reset => {
                    self.restart = true;
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
            }
        }
    }

    /// Wait until the next 1 ms tick is due. False when the program should end (stop or reset).
    fn wait_turn(&mut self) -> bool {
        loop {
            self.poll();
            if self.stop || self.restart {
                return false;
            }
            if self.paused {
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
        if !self.core.tick() {
            self.failed = true;
            self.publish(Some(UNSTABLE_MESSAGE));
            return Err(UNSTABLE_MESSAGE.into());
        }
        if self.core.due() {
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
        let s = self.0.borrow();
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

/// The sim thread body with a program: run it, then keep the simulation going until stopped. A
/// `Reset` restarts the simulation and the program.
pub(crate) fn sim_thread_with_program(
    mut core: SimCore,
    program: ProgramConfig,
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
        origin: Instant::now(),
        origin_sim: 0.0,
        channels: Vec::new(),
    }));
    inner.borrow_mut().publish(None);
    loop {
        {
            let mut s = inner.borrow_mut();
            if s.restart {
                s.core.reset();
                s.failed = false;
                s.restart = false;
                s.channels.clear();
                s.rebase();
                s.origin_sim = 0.0;
                s.publish(None);
            }
        }
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
        if inner.borrow().stop {
            return;
        }
        // The simulation carries on without a program until stopped or reset.
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
            if s.tick().is_err() {
                continue;
            }
        }
    }
}

fn load_message(e: &LoadError) -> String {
    e.to_string()
}
