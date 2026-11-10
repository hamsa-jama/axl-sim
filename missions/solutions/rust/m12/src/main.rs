// Reference solution for m12 (with the robot design in missions/solutions/m12.axlbot): drive up
// the ramp and stop on the top.
// The design does the real work. The heavy hub sits over the drive wheels and the caster stands
// well behind them, so the wheels keep their grip when the ramp shifts the weight backwards; the
// plate does not reach out far in front, so its nose clears the foot of the ramp.
// The program ramps the power up gently so the wheels do not spin, counts the distance with the
// wheel angles, and brakes when the top zone is reached (otherwise the robot drives off the edge).
use axl_robot::Robot;

const WHEEL_RADIUS_M: f64 = 0.028;
// Along the floor (0.25 m) and up the ramp to the middle of the top zone.
const STOP_AT_M: f64 = 1.30;
const POWER: f64 = 0.6;

fn main() {
    let mut robot = Robot::new();
    let left = robot.motor("left");
    let right = robot.motor("right");
    let start = 0.5 * (left.angle() + right.angle());
    let mut power = 0.0_f64;
    while robot.step(10) {
        let travelled = (0.5 * (left.angle() + right.angle()) - start) * WHEEL_RADIUS_M;
        if travelled >= STOP_AT_M {
            left.brake();
            right.brake();
            continue;
        }
        if power < POWER {
            power += 0.01;
        }
        left.power(power);
        right.power(power);
    }
}
