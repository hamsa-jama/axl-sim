// Reference solution for m07: a PI controller on the measured speed.
// The encoder's own speed reading is coarse (whole degrees every 10 ms), so the speed is measured
// from the angle instead: how far the wheels turned in the last 100 ms.
// Power = a guess + a part that grows with the error + a part that adds up the error over time
// (which removes the last little offset). The sum is clamped so it cannot wind up.
use axl_robot::Robot;

const WHEEL_RADIUS_M: f64 = 0.028;
const GOAL_MS: f64 = 0.25;
const DT: f64 = 0.01;
const WINDOW: usize = 10;

fn main() {
    let mut robot = Robot::new();
    let left = robot.motor("left");
    let right = robot.motor("right");
    let mut history = [0.0_f64; WINDOW];
    let mut n: usize = 0;
    let mut sum = 0.0_f64;
    let (kp, ki, base) = (1.2, 2.5, 0.35);
    while robot.step(10) {
        let angle = 0.5 * (left.angle() + right.angle());
        let old = if n >= WINDOW { history[n % WINDOW] } else { history[0] };
        history[n % WINDOW] = angle;
        n += 1;
        let speed = if n > WINDOW {
            (angle - old) * WHEEL_RADIUS_M / (WINDOW as f64 * DT)
        } else {
            0.0
        };
        let error = GOAL_MS - speed;
        sum = (sum + error * DT).clamp(-0.3, 0.3);
        let power = (base + kp * error + ki * sum).clamp(0.0, 1.0);
        left.power(power);
        right.power(power);
        robot.plot("speed", speed);
    }
}
