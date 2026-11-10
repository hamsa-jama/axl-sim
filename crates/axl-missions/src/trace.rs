//! What a graded run records.

use std::collections::BTreeMap;

/// The robot's state at one moment (the runner records one every 10 ms of simulated time).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    /// Simulated seconds.
    pub t: f64,
    /// Position of the robot's frame origin, metres.
    pub pos: [f64; 3],
    /// Yaw, radians, unwrapped (a full turn counts 2 pi).
    pub yaw: f64,
    /// Angle between the robot's up axis and the world's, radians.
    pub tilt: f64,
    /// Speed of the origin, m/s.
    pub speed: f64,
    /// Position of the robot's front point (along its heading), x then y, metres.
    pub front: [f64; 2],
    /// Distance from the front to the static geometry straight ahead, metres; negative for none.
    pub ahead_m: f64,
    /// A robot geom touches a wall now.
    pub wall_contact: bool,
}

/// Everything a run records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunTrace {
    /// The seed.
    pub seed: u64,
    /// Samples, oldest first.
    pub samples: Vec<Sample>,
    /// `axl_report` calls: (time, key, value).
    pub reports: Vec<(f64, String, f64)>,
    /// Numbers measured in the world at the start (`sensor_range_m`, ...).
    pub measured: BTreeMap<String, f64>,
    /// `axl_mark_pose` calls: (time, [x, y, yaw]).
    pub believed: Vec<(f64, [f64; 3])>,
    /// The program sent something to the plotter.
    pub plot_used: bool,
    /// Parts of the robot.
    pub part_count: u16,
    /// The program stopped with this message (error, watchdog, disabled API).
    pub program_error: Option<String>,
    /// The simulation became unstable.
    pub unstable: bool,
}

impl RunTrace {
    /// The first sample.
    pub fn first(&self) -> Option<&Sample> {
        self.samples.first()
    }

    /// The last sample.
    pub fn last(&self) -> Option<&Sample> {
        self.samples.last()
    }

    /// The last sample at or before `t` (or the first one).
    pub fn at(&self, t: f64) -> Option<&Sample> {
        self.samples
            .iter()
            .rev()
            .find(|s| s.t <= t + 1e-9)
            .or_else(|| self.samples.first())
    }
}
