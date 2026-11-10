// Reference solution for m08: follow the left edge of the line with a P controller. The colour
// sensor reads about 1 over the black line and about 78 over the white mat, so the edge is at
// about 40. Over white (too far left) the Rover steers right, over black it steers left.
use axl_robot::Robot;

const EDGE: f64 = 40.0;
const BASE_POWER: f64 = 0.22;
const GAIN: f64 = 0.35;

fn main() {
    let mut robot = Robot::new();
    let left = robot.motor("left");
    let right = robot.motor("right");
    let color = robot.color_sensor("color");
    while robot.step(10) {
        let error = (f64::from(color.reflect()) - EDGE) / EDGE; // -1 (black) to +1 (white)
        let turn = GAIN * error;
        left.power(BASE_POWER + turn);
        right.power(BASE_POWER - turn);
    }
}
