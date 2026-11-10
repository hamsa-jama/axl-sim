// Reference solution for m04: four times "drive 0.5 m, turn 90 degrees left". The program also
// keeps a believed pose from the commanded moves and marks it so AXL can draw it.
#[path = "../../common/motion.rs"]
mod motion;

use axl_robot::Robot;
use motion::{HALF_BASE_M, WHEEL_RADIUS_M, Wheels};
use std::f64::consts::PI;

struct Believed {
    x: f64,
    y: f64,
    yaw: f64,
}

fn drive(robot: &mut Robot, wheels: &mut Wheels, b: &mut Believed, metres: f64) -> bool {
    let a = metres / WHEEL_RADIUS_M;
    b.x += metres * b.yaw.cos();
    b.y += metres * b.yaw.sin();
    let ok = wheels.move_by(robot, a, a, 12.0, 15.0);
    robot.mark_pose(b.x, b.y, b.yaw);
    ok
}

fn turn_left(robot: &mut Robot, wheels: &mut Wheels, b: &mut Believed, angle: f64) -> bool {
    let a = HALF_BASE_M * angle / WHEEL_RADIUS_M;
    b.yaw += angle;
    let ok = wheels.move_by(robot, -a, a, 6.0, 10.0);
    robot.mark_pose(b.x, b.y, b.yaw);
    ok
}

fn main() {
    let mut robot = Robot::new();
    let mut wheels = Wheels::new(&robot);
    let mut believed = Believed { x: 0.0, y: 0.0, yaw: 0.0 };
    for _ in 0..4 {
        if !drive(&mut robot, &mut wheels, &mut believed, 0.5)
            || !turn_left(&mut robot, &mut wheels, &mut believed, PI / 2.0)
        {
            return;
        }
    }
    while robot.step(10) {}
}
