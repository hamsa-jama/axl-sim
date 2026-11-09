//! A recording robot for the host tests.

use axl_host::{
    ColorName, DeviceKind, LogStream, MotorCommand, MotorReading, RobotApi, StepStatus,
};

/// Devices: 0 hub, 1 left (motor), 2 right (motor), 3 eye (distance), 4 floor (colour), 5 bump (touch).
#[derive(Default)]
pub struct Recorder {
    pub calls: Vec<String>,
    pub plots: Vec<(String, f64)>,
    pub logs: Vec<(LogStream, String)>,
    pub ended: bool,
    pub over_after_steps: Option<u32>,
    pub steps: u32,
    pub time: f64,
}

const DEVICES: [(&str, DeviceKind); 6] = [
    ("hub", DeviceKind::Hub),
    ("left", DeviceKind::Motor),
    ("right", DeviceKind::Motor),
    ("eye", DeviceKind::Distance),
    ("floor", DeviceKind::Color),
    ("bump", DeviceKind::Touch),
];

impl RobotApi for Recorder {
    fn step(&mut self, ms: u32) -> Result<StepStatus, String> {
        self.calls.push(format!("step {ms}"));
        self.steps += 1;
        self.time += ms as f64 / 1000.0;
        if self.over_after_steps.is_some_and(|n| self.steps >= n) {
            Ok(StepStatus::MissionOver)
        } else {
            Ok(StepStatus::Running)
        }
    }
    fn time(&self) -> f64 {
        12.5 + self.time
    }
    fn find_device(&self, name: &str) -> Option<usize> {
        DEVICES.iter().position(|d| d.0 == name)
    }
    fn device_info(&self, dev: usize) -> Option<(String, DeviceKind)> {
        DEVICES.get(dev).map(|d| (d.0.to_owned(), d.1))
    }
    fn motor_command(&mut self, dev: usize, cmd: MotorCommand) {
        self.calls.push(format!("motor {dev} {cmd:?}"));
    }
    fn motor_reset_angle(&mut self, dev: usize) {
        self.calls.push(format!("reset_angle {dev}"));
    }
    fn motor_reading(&self, _dev: usize, what: MotorReading) -> f64 {
        match what {
            MotorReading::Angle => 1.25,
            MotorReading::Rate => 2.5,
            MotorReading::Current => 0.75,
            MotorReading::Done => 1.0,
        }
    }
    fn distance(&self, _dev: usize) -> f64 {
        0.321
    }
    fn color(&self, _dev: usize) -> ([i32; 3], i32, ColorName) {
        ([100, 200, 300], 42, ColorName::Red)
    }
    fn touch(&self, _dev: usize) -> (bool, f64) {
        (true, 2.5)
    }
    fn imu_gyro(&self) -> [f64; 3] {
        [1.0, 2.0, 3.0]
    }
    fn imu_accel(&self) -> [f64; 3] {
        [4.0, 5.0, 6.0]
    }
    fn battery_voltage(&self) -> f64 {
        7.7
    }
    fn plot(&mut self, channel: &str, value: f64) -> Result<(), String> {
        if self
            .plots
            .iter()
            .map(|p| &p.0)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 16
            && !self.plots.iter().any(|p| p.0 == channel)
        {
            return Err("axl_plot: a program can use at most 16 plot channels.".into());
        }
        self.plots.push((channel.to_owned(), value));
        Ok(())
    }
    fn mark_pose(&mut self, x: f64, y: f64, yaw: f64) {
        self.calls.push(format!("mark_pose {x} {y} {yaw}"));
    }
    fn draw_line(&mut self, from: [f64; 3], to: [f64; 3], rgba: u32) {
        self.calls
            .push(format!("draw_line {from:?} {to:?} {rgba:#x}"));
    }
    fn draw_clear(&mut self) {
        self.calls.push("draw_clear".into());
    }
    fn report(&mut self, key: &str, value: f64) {
        self.calls.push(format!("report {key} {value}"));
    }
    fn log(&mut self, stream: LogStream, line: &str) {
        self.logs.push((stream, line.to_owned()));
    }
    fn program_ended(&mut self) {
        self.ended = true;
    }
}
