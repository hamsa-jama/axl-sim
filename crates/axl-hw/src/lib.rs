//! axl-hw: the battery, DC motors with their built-in loops, encoders and sensors of an AXL robot
//! (SPEC.md, "Hardware models"), driven from the 1 ms step loop of `axl-sim`:
//!
//! 1. [`Hardware::pre_step`] turns motor commands into voltages, applies battery sag and sets `ctrl`.
//! 2. `Physics::step`.
//! 3. [`Hardware::post_step`] updates encoders and currents, takes sensor samples that are due
//!    (refreshing MuJoCo's derived values first) and adds noise.
//!
//! All noise comes from `ChaCha8Rng` seeded from the run seed and the device's port letter.
//! Nothing allocates in the step loop: queues are fixed arrays and every `Vec` is built in
//! [`Hardware::new`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod battery;
pub mod mat;
pub mod motor;
pub mod noise;
pub mod params;
pub mod queue;
pub mod rig;
pub mod sensors;

use axl_compile::DeviceMap;
use axl_compile::SensorRole;
use axl_mujoco::{ActuatorId, JointId, Physics, SensorId, SiteId};
use axl_parts::types::DeviceModel;

pub use battery::Battery;
pub use mat::MatTexture;
pub use motor::{Encoder, Motor, MotorCommand, MotorKind};
pub use noise::Noise;
pub use params::HwParams;
pub use sensors::{
    ColorName, ColorReading, ColorSensor, FloorMat, Imu, TimeOfFlight, TouchSensor, Ultrasonic,
};

/// What a device is, for handle checks ("device 3 (eye) is a distance sensor, not a motor").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    /// The controller hub.
    Hub,
    /// A motor or servo.
    Motor,
    /// Ultrasonic or time-of-flight distance sensor.
    Distance,
    /// Colour sensor.
    Color,
    /// Touch sensor.
    Touch,
}

impl DeviceKind {
    /// The words used in error messages, with the article.
    pub fn describe(self) -> &'static str {
        match self {
            DeviceKind::Hub => "the hub",
            DeviceKind::Motor => "a motor",
            DeviceKind::Distance => "a distance sensor",
            DeviceKind::Color => "a colour sensor",
            DeviceKind::Touch => "a touch sensor",
        }
    }
}

/// Where a device's model lives inside [`Hardware`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// The hub (battery and IMU).
    Hub,
    /// Index into the motors.
    Motor(usize),
    /// Index into the ultrasonic sensors.
    Ultrasonic(usize),
    /// Index into the time-of-flight sensors.
    Tof(usize),
    /// Index into the colour sensors.
    Color(usize),
    /// Index into the touch sensors.
    Touch(usize),
}

/// One named device of the robot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSlot {
    /// Name from the builder.
    pub name: String,
    /// Its kind.
    pub kind: DeviceKind,
    /// Where its model is.
    pub slot: Slot,
}

/// Run-wide options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HwOptions {
    /// Run seed; noise streams derive from it and the device's port letter.
    pub seed: u64,
    /// False switches off noise, bias and drift (the accuracy suite uses this).
    pub noise: bool,
    /// State of charge at the start, or `None` for the configured starting charge.
    pub start_soc: Option<f64>,
}

impl Default for HwOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            noise: true,
            start_soc: None,
        }
    }
}

/// All hardware models of one robot.
#[derive(Clone, Debug)]
pub struct Hardware {
    params: HwParams,
    opts: HwOptions,
    map: DeviceMap,
    battery: Battery,
    motors: Vec<Motor>,
    ultrasonics: Vec<Ultrasonic>,
    tofs: Vec<TimeOfFlight>,
    colors: Vec<ColorSensor>,
    touches: Vec<TouchSensor>,
    imu: Option<Imu>,
    devices: Vec<DeviceSlot>,
    floor: Option<FloorMat>,
    now_ms: u64,
    motor_current_abs: f64,
}

impl Hardware {
    /// Build the models for a compiled robot. `physics` must be the model compiled from the same
    /// MJCF (the encoders start at its joint angles).
    pub fn new(map: &DeviceMap, params: &HwParams, opts: HwOptions, physics: &Physics) -> Self {
        let mut hw = Self {
            params: *params,
            opts,
            map: map.clone(),
            battery: match opts.start_soc {
                Some(s) => Battery::with_soc(params.battery, s),
                None => Battery::new(params.battery),
            },
            motors: Vec::new(),
            ultrasonics: Vec::new(),
            tofs: Vec::new(),
            colors: Vec::new(),
            touches: Vec::new(),
            imu: None,
            devices: Vec::new(),
            floor: None,
            now_ms: 0,
            motor_current_abs: 0.0,
        };
        hw.build(physics);
        hw
    }

    fn build(&mut self, physics: &Physics) {
        let (params, opts) = (self.params, self.opts);
        let s = &params.sensors;
        let noise = |port: Option<axl_build::Port>, stream: u8| {
            let letter = port.map_or(b'H', |p| p.letter() as u8);
            Noise::new(opts.seed, letter, stream, opts.noise)
        };
        for d in &self.map.devices {
            let mut push = |kind, slot| {
                self.devices.push(DeviceSlot {
                    name: d.name.clone(),
                    kind,
                    slot,
                })
            };
            match d.kind {
                DeviceModel::Hub => {
                    let find = |role| {
                        d.sensors
                            .iter()
                            .find(|e| e.role == role)
                            .map(|e| SensorId(e.sensor.id))
                    };
                    if let (Some(g), Some(a)) = (find(SensorRole::Gyro), find(SensorRole::Accel)) {
                        self.imu = Some(Imu::new(
                            g,
                            a,
                            s.gyro,
                            s.accel,
                            noise(None, 0),
                            noise(None, 1),
                        ));
                    }
                    push(DeviceKind::Hub, Slot::Hub);
                }
                DeviceModel::MotorMedium | DeviceModel::MotorLarge | DeviceModel::Servo => {
                    if let (Some(mp), Some(a), Some(j)) = (&d.motor, &d.actuator, &d.joint) {
                        let mut m = Motor::new(
                            &d.name,
                            d.kind == DeviceModel::Servo,
                            *mp,
                            params.control,
                            &params.encoders,
                            ActuatorId(a.id),
                            JointId(j.id),
                        );
                        m.prime(physics);
                        push(DeviceKind::Motor, Slot::Motor(self.motors.len()));
                        self.motors.push(m);
                    }
                }
                DeviceModel::Ultrasonic => {
                    if let Some(site) = &d.site {
                        push(
                            DeviceKind::Distance,
                            Slot::Ultrasonic(self.ultrasonics.len()),
                        );
                        self.ultrasonics.push(Ultrasonic::new(
                            &d.name,
                            SiteId(site.id),
                            s.ultrasonic,
                            noise(d.port, 0),
                        ));
                    }
                }
                DeviceModel::TimeOfFlight => {
                    if let Some(site) = &d.site {
                        push(DeviceKind::Distance, Slot::Tof(self.tofs.len()));
                        self.tofs.push(TimeOfFlight::new(
                            &d.name,
                            SiteId(site.id),
                            s.tof,
                            noise(d.port, 0),
                        ));
                    }
                }
                DeviceModel::Color => {
                    if let Some(site) = &d.site {
                        push(DeviceKind::Color, Slot::Color(self.colors.len()));
                        self.colors.push(ColorSensor::new(
                            &d.name,
                            SiteId(site.id),
                            s.color,
                            &params.colors,
                            noise(d.port, 0),
                        ));
                    }
                }
                DeviceModel::Touch => {
                    if let Some(e) = d.sensors.iter().find(|e| e.role == SensorRole::Touch) {
                        push(DeviceKind::Touch, Slot::Touch(self.touches.len()));
                        self.touches.push(TouchSensor::new(
                            &d.name,
                            SensorId(e.sensor.id),
                            s.touch,
                            noise(d.port, 0),
                        ));
                    }
                }
            }
        }
    }

    /// Back to the start of the run (call together with `Physics::reset`). Noise streams restart
    /// from the seed, so a replay reproduces every reading.
    pub fn reset(&mut self, physics: &Physics) {
        let floor = self.floor.take();
        let map = std::mem::take(&mut self.map);
        *self = Self::new(&map, &self.params.clone(), self.opts, physics);
        self.floor = floor;
    }

    /// Set the floor mat the colour sensors read.
    pub fn set_floor_mat(&mut self, mat: Option<FloorMat>) {
        self.floor = mat;
    }

    /// Simulated time in whole milliseconds, counted by [`Hardware::post_step`].
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The battery.
    pub fn battery(&self) -> &Battery {
        &self.battery
    }

    /// Battery terminal voltage.
    pub fn battery_voltage(&self) -> f64 {
        self.battery.voltage()
    }

    /// All devices in uid order; a device's handle is its index here.
    pub fn devices(&self) -> &[DeviceSlot] {
        &self.devices
    }

    /// Handle of a device by name.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.devices.iter().position(|d| d.name == name)
    }

    /// A motor by name.
    pub fn motor(&self, name: &str) -> Option<&Motor> {
        self.motors.iter().find(|m| m.name() == name)
    }

    /// A motor by name, mutable (to command it).
    pub fn motor_mut(&mut self, name: &str) -> Option<&mut Motor> {
        self.motors.iter_mut().find(|m| m.name() == name)
    }

    /// Motor by index.
    pub fn motor_at(&self, i: usize) -> Option<&Motor> {
        self.motors.get(i)
    }

    /// Motor by index, mutable.
    pub fn motor_at_mut(&mut self, i: usize) -> Option<&mut Motor> {
        self.motors.get_mut(i)
    }

    /// All motors, mutable.
    pub fn motors_mut(&mut self) -> &mut [Motor] {
        &mut self.motors
    }

    /// All motors.
    pub fn motors(&self) -> &[Motor] {
        &self.motors
    }

    /// The hub IMU.
    pub fn imu(&self) -> Option<&Imu> {
        self.imu.as_ref()
    }

    /// The hub IMU, mutable.
    pub fn imu_mut(&mut self) -> Option<&mut Imu> {
        self.imu.as_mut()
    }

    /// Ultrasonic sensor by index.
    pub fn ultrasonic_at(&self, i: usize) -> Option<&Ultrasonic> {
        self.ultrasonics.get(i)
    }

    /// Time-of-flight sensor by index.
    pub fn tof_at(&self, i: usize) -> Option<&TimeOfFlight> {
        self.tofs.get(i)
    }

    /// Colour sensor by index.
    pub fn color_at(&self, i: usize) -> Option<&ColorSensor> {
        self.colors.get(i)
    }

    /// Touch sensor by index.
    pub fn touch_at(&self, i: usize) -> Option<&TouchSensor> {
        self.touches.get(i)
    }

    /// Distance as a controller sees it now (metres, or -1) for a distance-sensor slot.
    pub fn distance(&self, slot: Slot) -> Option<f64> {
        match slot {
            Slot::Ultrasonic(i) => self.ultrasonics.get(i).map(|s| s.distance(self.now_ms)),
            Slot::Tof(i) => self.tofs.get(i).map(|s| s.distance(self.now_ms)),
            _ => None,
        }
    }

    /// Step 1 of the loop: battery voltage, then motor commands to `ctrl`.
    pub fn pre_step(&mut self, physics: &mut Physics) {
        let vbat = self.battery.begin_step(self.motor_current_abs);
        for m in &mut self.motors {
            m.pre_step(physics, vbat);
        }
    }

    /// Step 3 of the loop: encoders, currents, charge, and any sensor samples that are due.
    pub fn post_step(&mut self, physics: &mut Physics) {
        self.now_ms += 1;
        let now = self.now_ms;
        let mut total = 0.0;
        for m in &mut self.motors {
            m.post_step(physics);
            total += m.current().abs();
        }
        self.motor_current_abs = total;
        self.battery.end_step(physics.timestep());
        for t in &mut self.touches {
            t.step(physics);
        }
        let imu_due = self
            .imu
            .as_ref()
            .is_some_and(|i| i.gyro_due(now) || i.accel_due(now));
        let any_due = imu_due
            || self.touches.iter().any(|s| s.due(now))
            || self.ultrasonics.iter().any(|s| s.due(now))
            || self.tofs.iter().any(|s| s.due(now))
            || self.colors.iter().any(|s| s.due(now));
        if !any_due {
            return;
        }
        // After mj_step, velocities, contacts and sensor values describe the start of the step.
        physics.forward();
        if let Some(imu) = &mut self.imu {
            if imu.gyro_due(now) {
                imu.sample_gyro(physics, now);
            }
            if imu.accel_due(now) {
                imu.sample_accel(physics, now);
            }
        }
        for s in &mut self.touches {
            if s.due(now) {
                s.sample(physics, now);
            }
        }
        for s in &mut self.ultrasonics {
            if s.due(now) {
                s.sample(physics, now);
            }
        }
        for s in &mut self.tofs {
            if s.due(now) {
                s.sample(physics, now);
            }
        }
        let mat = self.floor.as_ref();
        for s in &mut self.colors {
            if s.due(now) {
                s.sample(physics, now, mat);
            }
        }
    }
}
