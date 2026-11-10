// Reference solution for m03: to turn by an angle A on the spot, each wheel rolls along an arc of
// (wheel base / 2) * A in opposite directions.
#[path = "../../common/motion.rs"]
mod motion;

use axl_robot::Robot;
use motion::{HALF_BASE_M, WHEEL_RADIUS_M, Wheels};
use std::f64::consts::PI;

fn turn_left(robot: &mut Robot, wheels: &mut Wheels, angle: f64) -> bool {
    let a = HALF_BASE_M * angle / WHEEL_RADIUS_M;
    wheels.move_by(robot, -a, a, 6.0, 10.0)
}

fn main() {
    let mut robot = Robot::new();
    let mut wheels = Wheels::new(&robot);
    if turn_left(&mut robot, &mut wheels, PI / 2.0) && turn_left(&mut robot, &mut wheels, PI) {
        while robot.step(10) {}
    }
}
