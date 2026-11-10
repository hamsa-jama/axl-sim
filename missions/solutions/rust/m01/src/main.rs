// Reference solution for m01: drive 0.952 m (the front of the wheels is 48 mm ahead of the hub, so
// the front ends at 1.0 m) with a smooth move of the wheels, then stay put.
#[path = "../../common/motion.rs"]
mod motion;

use axl_robot::Robot;
use motion::{WHEEL_RADIUS_M, Wheels};

const DISTANCE_M: f64 = 0.952;

fn main() {
    let mut robot = Robot::new();
    let mut wheels = Wheels::new(&robot);
    let a = DISTANCE_M / WHEEL_RADIUS_M;
    if wheels.move_by(&mut robot, a, a, 12.0, 15.0) {
        while robot.step(10) {}
    }
}
