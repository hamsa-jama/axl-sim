//! What the controller can reach: the [`RobotApi`] trait. `axl-sim` implements it on top of the
//! physics and the hardware models; the tests implement it with a recorder.

pub use axl_hw::{ColorName, DeviceKind, MotorCommand};

/// How `axl_step` ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    /// The mission goes on (`AXL_RUNNING`).
    Running,
    /// The mission has ended (`AXL_MISSION_OVER`).
    MissionOver,
}

/// Which stream a log line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// A motor reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotorReading {
    /// Encoder angle, rad.
    Angle,
    /// Encoder speed, rad/s.
    Rate,
    /// Current, A.
    Current,
    /// 1.0 when a position command has arrived (or nothing is pending), else 0.0.
    Done,
}

/// The robot as the controller sees it. Device handles are indices into the robot's device list;
/// the host has already checked that `dev` exists and is of the right kind when it calls a method.
pub trait RobotApi {
    /// Run `ms` physics steps (1 to 1000) and pace against the wall clock. `Err` carries the message
    /// shown when the simulation cannot go on (for example, it became unstable).
    fn step(&mut self, ms: u32) -> Result<StepStatus, String>;
    /// Simulated seconds.
    fn time(&self) -> f64;
    /// Handle of the device called `name`.
    fn find_device(&self, name: &str) -> Option<usize>;
    /// Name and kind of a device, or `None` for a bad handle.
    fn device_info(&self, dev: usize) -> Option<(String, DeviceKind)>;
    /// Command a motor.
    fn motor_command(&mut self, dev: usize, cmd: MotorCommand);
    /// Make the motor's present angle zero.
    fn motor_reset_angle(&mut self, dev: usize);
    /// Read a motor.
    fn motor_reading(&self, dev: usize, what: MotorReading) -> f64;
    /// Distance in metres, or -1.
    fn distance(&self, dev: usize) -> f64;
    /// Colour sensor: RGB 0 to 1023, reflectance 0 to 100, colour name.
    fn color(&self, dev: usize) -> ([i32; 3], i32, ColorName);
    /// Touch sensor: pressed and force in newtons.
    fn touch(&self, dev: usize) -> (bool, f64);
    /// Hub gyroscope, rad/s, robot frame.
    fn imu_gyro(&self) -> [f64; 3];
    /// Hub accelerometer, m/s2, robot frame.
    fn imu_accel(&self) -> [f64; 3];
    /// Battery terminal voltage.
    fn battery_voltage(&self) -> f64;
    /// Add a point to a plot channel. `Err` stops the program with that message (for example, too
    /// many channels).
    fn plot(&mut self, channel: &str, value: f64) -> Result<(), String>;
    /// Record the robot's believed pose.
    fn mark_pose(&mut self, x: f64, y: f64, yaw: f64);
    /// Draw a line in the 3D view.
    fn draw_line(&mut self, from: [f64; 3], to: [f64; 3], rgba: u32);
    /// Clear the drawn lines.
    fn draw_clear(&mut self);
    /// A value the mission asked the robot to measure.
    fn report(&mut self, key: &str, value: f64);
    /// One line from stdout or stderr.
    fn log(&mut self, stream: LogStream, line: &str);
    /// `main` returned, or the program was stopped: brake every motor and run on.
    fn program_ended(&mut self);
}
