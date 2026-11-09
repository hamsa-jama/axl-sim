use axl_robot::Robot;

fn main() {
    let mut robot = Robot::new(); // calls axl_init(1)
    let left = robot.motor("left");
    let right = robot.motor("right");
    let eye = robot.distance_sensor("eye");
    while robot.step(10) {
        // false once the mission is over
        let p = match eye.distance() {
            Some(d) if d < 0.20 => 0.0,
            _ => 0.5,
        };
        left.power(p);
        right.power(p);
    }
}
