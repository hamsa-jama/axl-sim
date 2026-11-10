// Reference solution for m09: a PID line follower that slows down in corners.
// error = how far the colour sensor is from the edge of the line, -1 (over black) to +1 (over
// white). P reacts to the error, D to how fast it changes (it calms the wobble) and I to an
// error that stays on one side. The base power falls when the error is large, so the Rover slows
// into corners and speeds up on straights.
use axl_robot::Robot;

const EDGE: f64 = 40.0;
const DT: f64 = 0.01;
const KP: f64 = 0.8;
const KD: f64 = 1.5;
const KI: f64 = 0.2;
const FAST: f64 = 0.40;
const SLOW: f64 = 0.16;

fn main() {
    let mut robot = Robot::new();
    let left = robot.motor("left");
    let right = robot.motor("right");
    let color = robot.color_sensor("color");
    let (mut previous, mut sum) = (0.0_f64, 0.0_f64);
    while robot.step(10) {
        let error = (f64::from(color.reflect()) - EDGE) / EDGE;
        sum = (sum + error * DT).clamp(-0.5, 0.5);
        let derivative = (error - previous) / DT;
        previous = error;
        let turn = KP * error + KI * sum + KD * 0.01 * derivative;
        let base = FAST - (FAST - SLOW) * error.abs().min(1.0);
        left.power(base + turn);
        right.power(base - turn);
        robot.plot("error", error);
    }
}
