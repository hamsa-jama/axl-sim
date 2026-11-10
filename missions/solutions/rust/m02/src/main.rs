// Reference solution for m02: one turn of a 56 mm wheel rolls pi * 56 mm, so 0.75 m is
// 0.75 / 0.028 radians of wheel angle. A smooth move keeps the tyres from slipping, so the wheel
// angle is the distance whatever the battery.
#[path = "../../common/motion.rs"]
mod motion;

use axl_robot::Robot;
use motion::{WHEEL_RADIUS_M, Wheels};

fn main() {
    let mut robot = Robot::new();
    let mut wheels = Wheels::new(&robot);
    let a = 0.75 / WHEEL_RADIUS_M;
    if wheels.move_by(&mut robot, a, a, 12.0, 15.0) {
        while robot.step(10) {}
    }
}
