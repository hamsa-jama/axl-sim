//! axl-validate: the accuracy suite.
//!
//! Each test compares the physics (and, from milestone M6, the hardware
//! models) against a textbook answer. Expected values are computed in code
//! from the model's own parameters, never hard-coded, so a test checks the
//! physics rather than the arithmetic. See `SPEC.md`, "Accuracy validation
//! suite".

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod report;
pub mod scenes;
mod v_dynamics;
mod v_repro;

pub use report::{Row, TestResult, run_all, run_selected};

#[cfg(test)]
mod tests {
    use super::*;

    /// Every implemented check must pass; a failure prints the failing rows.
    #[test]
    fn the_accuracy_suite_passes() {
        for r in run_all() {
            let bad: Vec<_> = r.rows.iter().filter(|row| !row.pass).collect();
            assert!(r.passed(), "{} failed: {bad:#?}", r.id);
        }
    }
}
