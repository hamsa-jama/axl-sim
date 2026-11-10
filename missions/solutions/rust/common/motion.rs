//! Smooth wheel moves, shared by several solutions (`#[path]`-included, so each solution stays a
//! single crate). A trapezoid speed profile (accelerate, cruise, slow down) feeds a new target to
//! the motors' position loops every 10 ms. Starting and stopping gently keeps the tyres from
//! slipping and the Rover from tipping, so the wheel angles tell the truth about the distance.

#![allow(dead_code)]

use axl_robot::{Motor, Robot};

/// Seconds per profile step.
pub const STEP_S: f64 = 0.01;
/// Radius of the 56 mm wheels, metres.
pub const WHEEL_RADIUS_M: f64 = 0.028;
/// Half the distance between the drive wheels, metres.
pub const HALF_BASE_M: f64 = 0.0725;

/// The two drive motors and the angles they were last told to go to.
pub struct Wheels {
    pub left: Motor,
    pub right: Motor,
    left_target: f64,
    right_target: f64,
}

impl Wheels {
    /// Look up the motors called `left` and `right`.
    pub fn new(robot: &Robot) -> Self {
        Self {
            left: robot.motor("left"),
            right: robot.motor("right"),
            left_target: 0.0,
            right_target: 0.0,
        }
    }

    /// Move the wheels by `dl` and `dr` radians, together. Returns `false` if the mission ended.
    pub fn move_by(&mut self, robot: &mut Robot, dl: f64, dr: f64, max_speed: f64, accel: f64) -> bool {
        let dist = dl.abs().max(dr.abs());
        let (l0, r0) = (self.left_target, self.right_target);
        let (mut done, mut speed) = (0.0_f64, 0.0_f64);
        while done < dist {
            let slow = (2.0 * accel * (dist - done)).sqrt();
            speed += accel * STEP_S;
            speed = speed.min(max_speed).min(slow).max(0.3);
            done = (done + speed * STEP_S).min(dist);
            self.left_target = l0 + dl * done / dist;
            self.right_target = r0 + dr * done / dist;
            self.left.position(self.left_target, 30.0);
            self.right.position(self.right_target, 30.0);
            if !robot.step(10) {
                return false;
            }
        }
        // Settle: wait for both motors to arrive.
        for _ in 0..100 {
            if !robot.step(10) {
                return false;
            }
            if self.left.done() && self.right.done() {
                break;
            }
        }
        true
    }
}
