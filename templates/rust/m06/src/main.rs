// Mission 06: Filter the Noise. Read the mission brief, then write your program here.
use axl_robot::Robot;

fn main() {
    let mut robot = Robot::new(); // calls axl_init(1)
    let eye = robot.distance_sensor("eye");
    while robot.step(10) {
        // false once the mission is over
        let _ = eye; // read this sensor
    }
}
