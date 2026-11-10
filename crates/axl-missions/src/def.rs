//! The mission file format.

use serde::{Deserialize, Serialize};

/// Current `version` of the mission files.
pub const MISSION_VERSION: u32 = 1;

/// Which world, which variant, and which variant a given grading seed uses instead.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldRef {
    /// World id.
    pub id: String,
    /// Variant id.
    #[serde(default)]
    pub variant: Option<String>,
    /// `(seed, variant)` pairs that replace the variant for that seed (m05: seed 3 angles the wall).
    #[serde(default)]
    pub seed_variants: Vec<(u64, String)>,
}

impl WorldRef {
    /// The variant for `seed`.
    pub fn variant_for(&self, seed: u64) -> Option<&str> {
        self.seed_variants
            .iter()
            .find(|(s, _)| *s == seed)
            .map(|(_, v)| v.as_str())
            .or(self.variant.as_deref())
    }
}

/// Where the robot comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RobotSource {
    /// A starter robot (`rover`).
    Starter(String),
    /// The learner builds it.
    Build {
        /// Most parts allowed.
        max_parts: u16,
        /// Allowed part ids (empty: all).
        #[serde(default)]
        allowed: Vec<String>,
    },
}

/// Something that happens during a run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MissionEvent {
    /// An impulse on the robot's root body at time `t`.
    Push {
        /// Simulated seconds.
        t: f64,
        /// Impulse, N s, world frame.
        impulse_ns: [f64; 3],
    },
}

/// The expected value of a reported number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Expected {
    /// A fixed number.
    Value(f64),
    /// A number the runner measured in this world (a key of `RunTrace::measured`).
    Measured(String),
}

/// What a run must achieve. All criteria of a mission must hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Criterion {
    /// The robot's origin is inside the zone at some time up to `within_s`.
    ReachZone {
        /// Zone name.
        zone: String,
        /// Seconds.
        within_s: f64,
    },
    /// The final pose is near `(x, y, yaw)`.
    FinalPose {
        /// Metres.
        x: f64,
        /// Metres.
        y: f64,
        /// Radians.
        yaw: f64,
        /// Metres.
        pos_tol_m: f64,
        /// Radians.
        yaw_tol_rad: f64,
    },
    /// The final x is `x` within `tol_m` (and the robot has stopped), and the run took at most
    /// `within_s` to get within the tolerance and stay there.
    FinalX {
        /// Metres.
        x: f64,
        /// Metres.
        tol_m: f64,
        /// Seconds; `None` for no limit.
        #[serde(default)]
        within_s: Option<f64>,
    },
    /// The front of the robot has stopped between `min_x` and `max_x` by `within_s`.
    FrontStop {
        /// Metres.
        min_x: f64,
        /// Metres.
        max_x: f64,
        /// Seconds.
        within_s: f64,
    },
    /// The final unwrapped yaw is `yaw` within `tol_rad`, and the point midway between the drive
    /// wheels is within `drift_m` of where it started.
    TurnTo {
        /// Radians (a turn of 90 degrees then 180 degrees is 3 pi / 2).
        yaw: f64,
        /// Radians.
        tol_rad: f64,
        /// Metres.
        drift_m: f64,
    },
    /// The robot passes within `tol_m` of each point in order, then ends within `end_tol_m` of the
    /// start.
    Waypoints {
        /// Points, metres, relative to the start.
        points: Vec<[f64; 2]>,
        /// Metres.
        tol_m: f64,
        /// Metres.
        end_tol_m: f64,
    },
    /// No contact with geoms of this tag (`wall`) at any time.
    NoContact {
        /// Tag.
        tag: String,
    },
    /// The front ends `min_m` to `max_m` from the static geometry ahead, stopped, within `within_s`.
    FrontClearance {
        /// Metres.
        min_m: f64,
        /// Metres.
        max_m: f64,
        /// Seconds.
        within_s: f64,
    },
    /// Follow the guide line: never farther than `max_dev_m` from it, `laps` laps within
    /// `within_s`.
    FollowLine {
        /// Metres.
        max_dev_m: f64,
        /// Laps.
        laps: u8,
        /// Seconds.
        within_s: f64,
    },
    /// The mean speed between `from_s` and `to_s` is `target_ms` within `tol_ms`.
    MeanSpeed {
        /// Seconds.
        from_s: f64,
        /// Seconds.
        to_s: f64,
        /// Metres per second.
        target_ms: f64,
        /// Metres per second.
        tol_ms: f64,
    },
    /// A reported value matches.
    Report {
        /// Key given to `axl_report`.
        key: String,
        /// What it should be.
        expected: Expected,
        /// Tolerance.
        tol: f64,
        /// Seconds: the report must come by then (`None`: no limit).
        #[serde(default)]
        within_s: Option<f64>,
    },
    /// The robot never tilts more than `max_tilt_rad`.
    NoTip {
        /// Radians.
        max_tilt_rad: f64,
    },
    /// After a straight drive of `distance_m` along x: lateral error and yaw error at the end.
    HeadingLock {
        /// Metres.
        distance_m: f64,
        /// Metres.
        max_lateral_m: f64,
        /// Radians.
        max_yaw_rad: f64,
    },
    /// The believed pose (`axl_mark_pose`) at the end is within `tol_m` of the true pose.
    BelievedPose {
        /// Metres.
        tol_m: f64,
    },
    /// The final position is within `tol_m` of the start.
    EndNearStart {
        /// Metres.
        tol_m: f64,
    },
    /// The program sent something to the plotter.
    UsedPlot,
    /// No more than this many parts.
    MaxParts {
        /// Parts.
        n: u16,
    },
    /// The goal was reached within `within_s` (the first time in `zone`).
    FinishWithin {
        /// Zone name.
        zone: String,
        /// Seconds.
        within_s: f64,
    },
}

/// A star beyond the first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StarRule {
    /// What the learner reads.
    pub text: String,
    /// Extra criteria (the mission's own still hold).
    #[serde(default)]
    pub criteria: Vec<Criterion>,
    /// No hint may have been opened for this attempt.
    #[serde(default)]
    pub no_hints: bool,
}

/// The question asked before the first run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PredictPrompt {
    /// The question.
    pub question: String,
    /// Unit of the answer.
    pub unit: String,
    /// What the run shows: a key of `RunTrace::measured`, or `final_x` / `final_y`.
    pub measured: String,
}

/// A mission.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MissionDef {
    /// File format version.
    pub version: u32,
    /// Id (`m01`), equal to the file name.
    pub id: String,
    /// Title.
    pub title: String,
    /// Chapter.
    pub chapter: u8,
    /// Concepts taught.
    pub concepts: Vec<String>,
    /// The world.
    pub world: WorldRef,
    /// The robot.
    pub robot: RobotSource,
    /// Seconds before the run is cut off.
    pub time_limit_s: f64,
    /// Every seed must pass.
    pub grading_seeds: Vec<u64>,
    /// Start charge range (fraction), drawn per seed.
    pub battery_soc: (f64, f64),
    /// API functions switched off.
    #[serde(default)]
    pub disabled_api: Vec<String>,
    /// Events.
    #[serde(default)]
    pub events: Vec<MissionEvent>,
    /// The brief, Markdown.
    pub brief_md: String,
    /// The prediction asked first.
    #[serde(default)]
    pub predict: Option<PredictPrompt>,
    /// All must hold.
    pub criteria: Vec<Criterion>,
    /// The second and third stars.
    pub stars: [StarRule; 2],
    /// Three graduated hints.
    pub hints: [String; 3],
}

impl MissionDef {
    /// Parse RON text.
    pub fn from_ron(text: &str) -> Result<Self, String> {
        let m: Self = ron::de::from_str(text).map_err(|e| e.to_string())?;
        if m.version > MISSION_VERSION {
            return Err("This file was made by a newer version of AXL.".to_owned());
        }
        Ok(m)
    }

    /// Start charge for `seed`: drawn evenly from the range, the same for the same seed.
    pub fn soc_for(&self, seed: u64) -> f64 {
        let (lo, hi) = self.battery_soc;
        let x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11;
        lo + (hi - lo) * (x as f64 / (1u64 << 53) as f64)
    }
}
