// Mission 07: Hold the Speed. Read the mission brief, then write your program here.
use axl_robot::Robot;

fn main() {
    let mut robot = Robot::new(); // calls axl_init(1)
    let left = robot.motor("left");
    let right = robot.motor("right");
    while robot.step(10) {
        // false once the mission is over
        left.power(0.0);
        right.power(0.0);
    }
}
