// Reference solution for m06: average 20 readings (one every 50 ms) and report the mean.
use axl_robot::Robot;

fn main() {
    let mut robot = Robot::new();
    let eye = robot.distance_sensor("eye");
    let (mut sum, mut n) = (0.0, 0);
    robot.step(100); // the first reading arrives after the sensor's delay
    for _ in 0..20 {
        robot.step(50);
        if let Some(d) = eye.distance() {
            sum += d;
            n += 1;
        }
    }
    if n > 0 {
        robot.report("distance", sum / f64::from(n));
    }
    while robot.step(50) {}
}
