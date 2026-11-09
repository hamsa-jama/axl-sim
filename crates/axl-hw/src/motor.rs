//! DC motors with their built-in loops and encoders (SPEC "DC motor model", "Motor command
//! modes", "Motor parameters" and "Encoders").

use std::f64::consts::{FRAC_PI_2, TAU};

use axl_compile::params::MotorParams;
use axl_mujoco::{ActuatorId, JointId, Physics};

use crate::params::{ControlParams, EncoderParams};

/// What a motor was last told to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotorCommand {
    /// Duty cycle p in [-1, 1]: V = p x battery voltage.
    Power(f64),
    /// Built-in PI loop on encoder speed, rad/s.
    Speed(f64),
    /// P loop on angle (rad, in the encoder frame) giving a speed target limited to `max_speed`.
    Position {
        /// Target angle, rad.
        target: f64,
        /// Largest speed, rad/s.
        max_speed: f64,
    },
    /// V = 0; back-EMF brakes the motor.
    Brake,
    /// V = the motor's own back-EMF, so current and torque are about zero.
    Coast,
    /// A fixed voltage, limited by the battery. For the sandbox and the accuracy suite; it is not
    /// part of the controller ABI.
    Volts(f64),
}

impl MotorCommand {
    fn same_mode(&self, other: &MotorCommand) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// Which motor this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotorKind {
    /// Medium or large continuous motor.
    Continuous,
    /// 180 degree servo with a built-in position loop.
    Servo,
}

/// Incremental encoder: whole counts only, speed as the change over a window.
#[derive(Clone, Debug)]
pub struct Encoder {
    counts_per_rad: f64,
    zero: i64,
    raw: i64,
    hist: [i64; 64],
    pos: usize,
    window: usize,
    speed: f64,
}

impl Encoder {
    /// An encoder with `counts_per_rad` resolution; speed is taken over `window_ms` steps.
    pub fn new(counts_per_rad: f64, window_ms: usize) -> Self {
        Self {
            counts_per_rad,
            zero: 0,
            raw: 0,
            hist: [0; 64],
            pos: 0,
            window: window_ms.clamp(1, 63),
            speed: 0.0,
        }
    }

    /// Counts since the last zeroing: floor(theta x counts per rad).
    pub fn counts(&self) -> i64 {
        self.raw - self.zero
    }

    /// Angle in rad, a whole number of counts.
    pub fn angle(&self) -> f64 {
        self.counts() as f64 / self.counts_per_rad
    }

    /// Speed in rad/s: the change in counts over the last window.
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Counts per radian.
    pub fn counts_per_rad(&self) -> f64 {
        self.counts_per_rad
    }

    /// Make the present position the zero.
    pub fn reset(&mut self) {
        self.zero = self.raw;
    }

    /// Take the joint angle of this step (called once per 1 ms step).
    pub fn update(&mut self, theta: f64, dt: f64) {
        self.raw = (theta * self.counts_per_rad).floor() as i64;
        let n = self.hist.len();
        let old = self.hist[(self.pos + n - self.window) % n];
        self.hist[self.pos] = self.raw;
        self.pos = (self.pos + 1) % n;
        self.speed = (self.raw - old) as f64 / self.counts_per_rad / (self.window as f64 * dt);
    }

    /// Start from a known angle (no speed).
    pub fn prime(&mut self, theta: f64) {
        self.raw = (theta * self.counts_per_rad).floor() as i64;
        self.hist = [self.raw; 64];
        self.speed = 0.0;
    }
}

/// One motor: command mode, loops, encoder and current.
#[derive(Clone, Debug)]
pub struct Motor {
    name: String,
    kind: MotorKind,
    p: MotorParams,
    ctl: ControlParams,
    actuator: ActuatorId,
    joint: JointId,
    cmd: MotorCommand,
    integ: f64,
    enc: Encoder,
    volts: f64,
    current: f64,
    omega: f64,
}

impl Motor {
    /// Build a motor. `servo` selects the servo encoder (1024 counts over 300 degrees) and the
    /// +-90 degree position limit.
    pub fn new(
        name: &str,
        servo: bool,
        p: MotorParams,
        ctl: ControlParams,
        enc: &EncoderParams,
        actuator: ActuatorId,
        joint: JointId,
    ) -> Self {
        let counts_per_rad = if servo {
            enc.servo_counts / enc.servo_span_deg.to_radians()
        } else {
            enc.cpr / TAU
        };
        let mut m = Self {
            name: name.to_owned(),
            kind: if servo {
                MotorKind::Servo
            } else {
                MotorKind::Continuous
            },
            p,
            ctl,
            actuator,
            joint,
            cmd: MotorCommand::Brake,
            integ: 0.0,
            enc: Encoder::new(counts_per_rad, ctl.speed_window_ms),
            volts: 0.0,
            current: 0.0,
            omega: 0.0,
        };
        if servo {
            // A servo holds its position until told otherwise.
            m.cmd = MotorCommand::Position {
                target: 0.0,
                max_speed: p.omega0,
            };
        }
        m
    }

    /// Device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Continuous motor or servo.
    pub fn kind(&self) -> MotorKind {
        self.kind
    }

    /// Motor parameters.
    pub fn params(&self) -> &MotorParams {
        &self.p
    }

    /// The MuJoCo actuator.
    pub fn actuator(&self) -> ActuatorId {
        self.actuator
    }

    /// The MuJoCo joint.
    pub fn joint(&self) -> JointId {
        self.joint
    }

    /// The active command.
    pub fn command_mode(&self) -> MotorCommand {
        self.cmd
    }

    /// Give the motor a new command. The speed loop's integral restarts only when the mode changes.
    pub fn command(&mut self, mut cmd: MotorCommand) {
        match &mut cmd {
            MotorCommand::Power(p) => *p = p.clamp(-1.0, 1.0),
            MotorCommand::Position { target, max_speed } => {
                *max_speed = max_speed.abs();
                if self.kind == MotorKind::Servo {
                    *target = target.clamp(-FRAC_PI_2, FRAC_PI_2);
                }
            }
            _ => {}
        }
        if !self.cmd.same_mode(&cmd) {
            self.integ = 0.0;
        }
        self.cmd = cmd;
    }

    /// Make the present position the encoder's zero.
    pub fn reset_angle(&mut self) {
        self.enc.reset();
    }

    /// Encoder counts since the last reset.
    pub fn counts(&self) -> i64 {
        self.enc.counts()
    }

    /// Encoder angle, rad (a whole number of counts).
    pub fn angle(&self) -> f64 {
        self.enc.angle()
    }

    /// Encoder speed, rad/s (change in counts over the last 10 ms).
    pub fn rate(&self) -> f64 {
        self.enc.speed()
    }

    /// Motor current, A (signed): I = (V - V_nom x omega / omega_0) / R.
    pub fn current(&self) -> f64 {
        self.current
    }

    /// Voltage applied in the last step.
    pub fn applied_volts(&self) -> f64 {
        self.volts
    }

    /// True when a position command has arrived: error under 1 degree and speed under 0.05 rad/s.
    /// Other modes have nothing pending, so they report true.
    pub fn done(&self) -> bool {
        match self.cmd {
            MotorCommand::Position { target, .. } => {
                (target - self.enc.angle()).abs() < self.ctl.done_error_deg.to_radians()
                    && self.enc.speed().abs() < self.ctl.done_speed
            }
            _ => true,
        }
    }

    fn pi(&mut self, reference: f64, vbat: f64, dt: f64) -> f64 {
        let e = reference - self.enc.speed();
        let ff = self.p.v_nom / self.p.omega0 * reference;
        let u = ff + self.ctl.speed_kp * e + self.ctl.speed_ki * self.integ;
        let sat = u.clamp(-vbat, vbat);
        // Anti-windup: stop integrating while saturated unless the error pulls back.
        if u == sat || (u > sat && e < 0.0) || (u < sat && e > 0.0) {
            self.integ += e * dt;
        }
        sat
    }

    /// Before the physics step: turn the command into a voltage and set `ctrl`.
    /// `vbat` is the battery terminal voltage.
    pub fn pre_step(&mut self, physics: &mut Physics, vbat: f64) {
        let dt = physics.timestep();
        let v = match self.cmd {
            MotorCommand::Power(p) => p * vbat,
            MotorCommand::Volts(v) => v.clamp(-vbat, vbat),
            MotorCommand::Brake => 0.0,
            MotorCommand::Coast => {
                (self.p.v_nom * physics.joint_vel(self.joint) / self.p.omega0).clamp(-vbat, vbat)
            }
            MotorCommand::Speed(w) => self.pi(w, vbat, dt),
            MotorCommand::Position { target, max_speed } => {
                let w = (self.ctl.position_kp * (target - self.enc.angle()))
                    .clamp(-max_speed, max_speed);
                self.pi(w, vbat, dt)
            }
        };
        self.volts = v;
        physics.set_ctrl(self.actuator, v);
    }

    /// After the physics step: read the joint, update the encoder and the current.
    pub fn post_step(&mut self, physics: &Physics) {
        let dt = physics.timestep();
        self.omega = physics.joint_vel(self.joint);
        self.enc.update(physics.joint_pos(self.joint), dt);
        self.current = (self.volts - self.p.v_nom * self.omega / self.p.omega0) / self.p.resistance;
    }

    /// Start state: encoder primed to the joint's present angle.
    pub fn prime(&mut self, physics: &Physics) {
        self.enc.prime(physics.joint_pos(self.joint));
        self.omega = physics.joint_vel(self.joint);
    }

    /// Back to the start (after `Physics::reset`).
    pub fn reset(&mut self, physics: &Physics) {
        self.integ = 0.0;
        self.volts = 0.0;
        self.current = 0.0;
        self.enc.zero = 0;
        self.prime(physics);
        self.cmd = if self.kind == MotorKind::Servo {
            MotorCommand::Position {
                target: 0.0,
                max_speed: self.p.omega0,
            }
        } else {
            MotorCommand::Brake
        };
    }
}
