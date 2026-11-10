//! axl-missions: mission definitions, success evaluators, scoring and progress (SPEC.md,
//! "Missions and curriculum").
//!
//! A mission is a RON file in `assets/missions/`. A graded run produces a [`trace::RunTrace`]
//! (the robot's state every 10 ms of simulated time, plus reports and events); [`eval`] checks the
//! criteria against it and says which one failed, by how much and on which seed.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod def;
pub mod eval;
pub mod library;
pub mod progress;
pub mod trace;

pub use def::{
    Criterion, Expected, MissionDef, MissionEvent, PredictPrompt, RobotSource, StarRule, WorldRef,
};
pub use eval::{CriterionResult, EvalContext, SeedResult, evaluate_seed, grade};
pub use library::{MissionError, builtin_missions};
pub use trace::{RunTrace, Sample};
