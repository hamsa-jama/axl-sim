// Reference solution for m10: keep the heading with the gyroscope, and dead-reckon the sideways
// position.
// 1. Stand still for 0.4 s and average the gyro's z reading: that is its bias (the pushes only
//    start at 0.5 s).
// 2. Add up (gyro z - bias) x dt: that is the heading, in radians, positive to the left.
// 3. Every step the Rover rolls forward by ds along its heading, which moves it sideways by
//    ds x sin(heading). Add that up too: that is how far to the side it believes it is.
// 4. Steer towards a heading that brings the sideways error back to zero.
use axl_robot::Robot;

const WHEEL_RADIUS_M: f64 = 0.028;
const GOAL_M: f64 = 1.5;
const DT: f64 = 0.01;
const KH: f64 = 40.0;
const KY: f64 = 3.0;
const SPEED: f64 = 0.25;

fn main() {
    let mut robot = Robot::new();
    let left = robot.motor("left");
    let right = robot.motor("right");
    let imu = robot.imu();

    let samples = 40;
    let mut bias = 0.0;
    for _ in 0..samples {
        if !robot.step(10) {
            return;
        }
        bias += imu.gyro()[2] / f64::from(samples);
    }

    let (mut heading, mut side, mut travelled) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut last_l, mut last_r) = (left.angle(), right.angle());
    while robot.step(10) {
        heading += (imu.gyro()[2] - bias) * DT;
        let (l, r) = (left.angle(), right.angle());
        let ds = 0.5 * ((l - last_l) + (r - last_r)) * WHEEL_RADIUS_M;
        last_l = l;
        last_r = r;
        travelled += ds;
        side += ds * heading.sin();

        let remaining = GOAL_M - travelled;
        let mut speed = SPEED;
        if remaining < 0.2 {
            speed = SPEED * remaining / 0.2; // slow down gently
        }
        if remaining < 0.004 {
            speed = 0.0;
        } else if speed < 0.05 {
            speed = 0.05;
        }

        let wanted = (-KY * side).clamp(-0.15, 0.15); // to the left of the line: aim right
        // A positive heading error means the Rover points too far left: the left wheel speeds up.
        let steer = KH * (heading - wanted);
        left.speed(speed / WHEEL_RADIUS_M + steer);
        right.speed(speed / WHEEL_RADIUS_M - steer);
        robot.plot("heading_deg", heading.to_degrees());
        robot.plot("side_mm", side * 1000.0);
    }
}
