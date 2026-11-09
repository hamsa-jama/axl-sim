//! The Rust interface to the AXL robot simulator. It mirrors `sdk/c/axl.h` function for function.
//!
//! A program is an ordinary Rust program built for `wasm32-wasip1`. Its `main` loops, calling
//! [`Robot::step`] to let time pass, like firmware on a real robot:
//!
//! ```no_run
//! use axl_robot::Robot;
//!
//! fn main() {
//!     let mut robot = Robot::new();            // calls axl_init(1)
//!     let left = robot.motor("left");
//!     let right = robot.motor("right");
//!     let eye = robot.distance_sensor("eye");
//!     while robot.step(10) {                   // false once the mission is over
//!         let p = match eye.distance() {
//!             Some(d) if d < 0.20 => 0.0,
//!             _ => 0.5,
//!         };
//!         left.power(p);
//!         right.power(p);
//!     }
//! }
//! ```
//!
//! Functions that return `-1` in C return `Option` here. Looking up a device that does not exist
//! panics with a message that names it. Units are SI: metres, radians, seconds, newtons, volts.

/// The interface version this crate speaks (`AXL_ABI_VERSION` in `axl.h`).
pub const ABI_VERSION: i32 = 1;

/// Raw imports from the `axl` module. Use the wrappers below instead.
#[allow(missing_docs)]
pub mod raw {
    #[cfg_attr(target_arch = "wasm32", link(wasm_import_module = "axl"))]
    unsafe extern "C" {
        #[cfg_attr(target_arch = "wasm32", link_name = "init")]
        pub fn axl_init(abi: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "step")]
        pub fn axl_step(ms: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "time")]
        pub fn axl_time() -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "device")]
        pub fn axl_device(name: *const u8, len: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "device_kind")]
        pub fn axl_device_kind(dev: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_power")]
        pub fn axl_motor_power(dev: i32, p: f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_speed")]
        pub fn axl_motor_speed(dev: i32, rad_s: f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_position")]
        pub fn axl_motor_position(dev: i32, rad: f64, max_rad_s: f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_brake")]
        pub fn axl_motor_brake(dev: i32);
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_coast")]
        pub fn axl_motor_coast(dev: i32);
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_reset_angle")]
        pub fn axl_motor_reset_angle(dev: i32);
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_angle")]
        pub fn axl_motor_angle(dev: i32) -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_rate")]
        pub fn axl_motor_rate(dev: i32) -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_current")]
        pub fn axl_motor_current(dev: i32) -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "motor_done")]
        pub fn axl_motor_done(dev: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "distance")]
        pub fn axl_distance(dev: i32) -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "color_rgb")]
        pub fn axl_color_rgb(dev: i32, rgb3: *mut i32);
        #[cfg_attr(target_arch = "wasm32", link_name = "color_reflect")]
        pub fn axl_color_reflect(dev: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "color_name")]
        pub fn axl_color_name(dev: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "touch_pressed")]
        pub fn axl_touch_pressed(dev: i32) -> i32;
        #[cfg_attr(target_arch = "wasm32", link_name = "touch_force")]
        pub fn axl_touch_force(dev: i32) -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "imu_gyro")]
        pub fn axl_imu_gyro(xyz3: *mut f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "imu_accel")]
        pub fn axl_imu_accel(xyz3: *mut f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "battery_voltage")]
        pub fn axl_battery_voltage() -> f64;
        #[cfg_attr(target_arch = "wasm32", link_name = "plot")]
        pub fn axl_plot(channel: *const u8, len: i32, value: f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "mark_pose")]
        pub fn axl_mark_pose(x: f64, y: f64, yaw: f64);
        #[cfg_attr(target_arch = "wasm32", link_name = "draw_line")]
        pub fn axl_draw_line(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64, rgba: u32);
        #[cfg_attr(target_arch = "wasm32", link_name = "draw_clear")]
        pub fn axl_draw_clear();
        #[cfg_attr(target_arch = "wasm32", link_name = "report")]
        pub fn axl_report(key: *const u8, len: i32, value: f64);
    }
}

/// The colour names a [`ColorSensor`] can report (`AXL_COLOR_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorName {
    /// Nothing in range.
    None,
    /// Black.
    Black,
    /// White.
    White,
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// Yellow.
    Yellow,
}

impl ColorName {
    fn from_code(c: i32) -> Self {
        match c {
            1 => Self::Black,
            2 => Self::White,
            3 => Self::Red,
            4 => Self::Green,
            5 => Self::Blue,
            6 => Self::Yellow,
            _ => Self::None,
        }
    }
}

/// What kind of device a handle is (`AXL_KIND_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The controller hub.
    Hub,
    /// A motor or servo.
    Motor,
    /// A distance sensor.
    Distance,
    /// A colour sensor.
    Color,
    /// A touch sensor.
    Touch,
}

/// The robot: the one value your program creates first.
pub struct Robot {
    _private: (),
}

impl Robot {
    /// Start talking to the simulator (calls `axl_init`).
    pub fn new() -> Self {
        // SAFETY: plain call into the host; no pointers.
        unsafe { raw::axl_init(ABI_VERSION) };
        Self { _private: () }
    }

    /// Let `ms` milliseconds pass (1 to 1000). Returns `false` once the mission is over.
    pub fn step(&mut self, ms: i32) -> bool {
        // SAFETY: plain call into the host; no pointers.
        unsafe { raw::axl_step(ms) == 0 }
    }

    /// Simulated time in seconds since the start of the run.
    pub fn time(&self) -> f64 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_time() }
    }

    fn handle(&self, name: &str) -> i32 {
        // SAFETY: the pointer and length describe `name`, which outlives the call.
        let h = unsafe { raw::axl_device(name.as_ptr(), name.len() as i32) };
        if h < 0 {
            panic!("axl-robot: there is no device called {name:?}. Check the name you gave it in the builder.");
        }
        h
    }

    /// The device called `name`, or `None` if there is none.
    pub fn find(&self, name: &str) -> Option<(i32, Kind)> {
        // SAFETY: the pointer and length describe `name`.
        let h = unsafe { raw::axl_device(name.as_ptr(), name.len() as i32) };
        if h < 0 {
            return None;
        }
        // SAFETY: plain call into the host.
        let kind = match unsafe { raw::axl_device_kind(h) } {
            0 => Kind::Hub,
            1 => Kind::Motor,
            2 => Kind::Distance,
            3 => Kind::Color,
            _ => Kind::Touch,
        };
        Some((h, kind))
    }

    /// The motor or servo called `name`. Panics if there is no such device.
    pub fn motor(&self, name: &str) -> Motor {
        Motor(self.handle(name))
    }

    /// The distance sensor called `name`. Panics if there is no such device.
    pub fn distance_sensor(&self, name: &str) -> DistanceSensor {
        DistanceSensor(self.handle(name))
    }

    /// The colour sensor called `name`. Panics if there is no such device.
    pub fn color_sensor(&self, name: &str) -> ColorSensor {
        ColorSensor(self.handle(name))
    }

    /// The touch sensor called `name`. Panics if there is no such device.
    pub fn touch_sensor(&self, name: &str) -> TouchSensor {
        TouchSensor(self.handle(name))
    }

    /// The hub's gyroscope and accelerometer.
    pub fn imu(&self) -> Imu {
        Imu(())
    }

    /// The battery voltage right now. It sags when the motors draw current.
    pub fn battery_voltage(&self) -> f64 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_battery_voltage() }
    }

    /// Send a value to a named channel of the plotter (up to 16 channels).
    pub fn plot(&self, channel: &str, value: f64) {
        // SAFETY: the pointer and length describe `channel`.
        unsafe { raw::axl_plot(channel.as_ptr(), channel.len() as i32, value) }
    }

    /// Tell the simulator where the robot believes it is (metres, radians).
    pub fn mark_pose(&self, x: f64, y: f64, yaw: f64) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_mark_pose(x, y, yaw) }
    }

    /// Draw a line in the 3D view with colour `0xRRGGBBAA`.
    pub fn draw_line(&self, from: [f64; 3], to: [f64; 3], rgba: u32) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_draw_line(from[0], from[1], from[2], to[0], to[1], to[2], rgba) }
    }

    /// Remove every line drawn with [`Robot::draw_line`].
    pub fn draw_clear(&self) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_draw_clear() }
    }

    /// Report a measured value to a mission that asks for one.
    pub fn report(&self, key: &str, value: f64) {
        // SAFETY: the pointer and length describe `key`.
        unsafe { raw::axl_report(key.as_ptr(), key.len() as i32, value) }
    }
}

impl Default for Robot {
    fn default() -> Self {
        Self::new()
    }
}

/// A motor or servo.
#[derive(Clone, Copy, Debug)]
pub struct Motor(i32);

impl Motor {
    /// Drive with power `p` from -1 to 1 (the voltage is `p` times the battery voltage).
    pub fn power(&self, p: f64) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_power(self.0, p) }
    }
    /// Hold a speed in rad/s with the motor's built-in PI loop.
    pub fn speed(&self, rad_s: f64) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_speed(self.0, rad_s) }
    }
    /// Turn to an angle in radians, never faster than `max_rad_s`. Check [`Motor::done`].
    pub fn position(&self, rad: f64, max_rad_s: f64) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_position(self.0, rad, max_rad_s) }
    }
    /// Brake: zero voltage, back-EMF slows the motor.
    pub fn brake(&self) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_brake(self.0) }
    }
    /// Coast: spin freely with about zero current.
    pub fn coast(&self) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_coast(self.0) }
    }
    /// Make the present angle the new zero of [`Motor::angle`].
    pub fn reset_angle(&self) {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_reset_angle(self.0) }
    }
    /// Encoder angle in radians (whole degrees).
    pub fn angle(&self) -> f64 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_angle(self.0) }
    }
    /// Encoder speed in rad/s (change in counts over the last 10 ms).
    pub fn rate(&self) -> f64 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_rate(self.0) }
    }
    /// Motor current in amperes.
    pub fn current(&self) -> f64 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_current(self.0) }
    }
    /// True when a position command has arrived, or nothing is pending.
    pub fn done(&self) -> bool {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_motor_done(self.0) != 0 }
    }
}

/// An ultrasonic or time-of-flight distance sensor.
#[derive(Clone, Copy, Debug)]
pub struct DistanceSensor(i32);

impl DistanceSensor {
    /// Distance in metres, or `None` when nothing is in range.
    pub fn distance(&self) -> Option<f64> {
        // SAFETY: plain call into the host.
        let d = unsafe { raw::axl_distance(self.0) };
        (d >= 0.0).then_some(d)
    }
}

/// A colour sensor.
#[derive(Clone, Copy, Debug)]
pub struct ColorSensor(i32);

impl ColorSensor {
    /// Red, green and blue, each 0 to 1023.
    pub fn rgb(&self) -> [i32; 3] {
        let mut v = [0i32; 3];
        // SAFETY: `v` has room for the three values the host writes.
        unsafe { raw::axl_color_rgb(self.0, v.as_mut_ptr()) };
        v
    }
    /// Reflected light, 0 to 100.
    pub fn reflect(&self) -> i32 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_color_reflect(self.0) }
    }
    /// The nearest colour name.
    pub fn name(&self) -> ColorName {
        // SAFETY: plain call into the host.
        ColorName::from_code(unsafe { raw::axl_color_name(self.0) })
    }
}

/// A touch sensor.
#[derive(Clone, Copy, Debug)]
pub struct TouchSensor(i32);

impl TouchSensor {
    /// True while the plunger is pressed.
    pub fn pressed(&self) -> bool {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_touch_pressed(self.0) != 0 }
    }
    /// Force on the plunger in newtons, 0 to 10.
    pub fn force(&self) -> f64 {
        // SAFETY: plain call into the host.
        unsafe { raw::axl_touch_force(self.0) }
    }
}

/// The hub's gyroscope and accelerometer (robot frame).
#[derive(Clone, Copy, Debug)]
pub struct Imu(());

impl Imu {
    /// Gyroscope reading in rad/s.
    pub fn gyro(&self) -> [f64; 3] {
        let mut v = [0.0f64; 3];
        // SAFETY: `v` has room for the three values the host writes.
        unsafe { raw::axl_imu_gyro(v.as_mut_ptr()) };
        v
    }
    /// Accelerometer reading in m/s^2 (about 9.8 on z at rest).
    pub fn accel(&self) -> [f64; 3] {
        let mut v = [0.0f64; 3];
        // SAFETY: `v` has room for the three values the host writes.
        unsafe { raw::axl_imu_accel(v.as_mut_ptr()) };
        v
    }
}
