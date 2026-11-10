// Reference solution for m05: the time-of-flight sensor sees the wall even when it is turned 50
// degrees (the ultrasonic one does not), so drive on its reading. The speed is proportional to how
// far the Rover still has to go, which brings it to a gentle stop.
use axl_robot::Robot;

const WHEEL_RADIUS_M: f64 = 0.028;
// Measured: with the front of the wheels against a wall this sensor reads 50 mm.
const SENSOR_TO_FRONT_M: f64 = 0.050;
const WANTED_GAP_M: f64 = 0.100;

fn main() {
    let mut robot = Robot::new();
    let left = robot.motor("left");
    let right = robot.motor("right");
    let tof = robot.distance_sensor("tof");
    while robot.step(10) {
        let mut speed = 0.3; // m/s while nothing is in range yet
        if let Some(d) = tof.distance() {
            let error = d - (WANTED_GAP_M + SENSOR_TO_FRONT_M);
            speed = (1.2 * error).clamp(-0.1, 0.45);
            // Very slow targets do not move the wheels at all, so creep a little faster.
            if speed > 0.0 && speed < 0.03 {
                speed = 0.03;
            }
            if error.abs() < 0.0015 {
                speed = 0.0;
            }
        }
        left.speed(speed / WHEEL_RADIUS_M);
        right.speed(speed / WHEEL_RADIUS_M);
    }
}
