// Reference solution for m11: keep one hand on the wall (the right-hand rule), as a state machine.
// At every cell the Rover looks right, then straight, then left; it takes the first free side and
// otherwise turns around. It always knows that the cell it came from is free, so it never looks
// back. Looking means turning the Rover and reading the distance sensor: a reading under
// 0.2 m is a wall in the way.
//
// Moves are smooth: a trapezoid speed profile feeds a new target to the motors' position loops
// every 10 ms. Turns follow the gyro's heading, so that small turning errors do not add up over
// 40 cells, and every look also tells the Rover where it stands in its cell.
use axl_robot::{DistanceSensor, Imu, Motor, Robot};
use std::f64::consts::PI;

const STEP_S: f64 = 0.01;
const WHEEL_RADIUS_M: f64 = 0.028;
// Half the distance between the drive wheels, as the Rover really turns (measured: a 90 degree
// turn computed with 0.0725 comes out as 91.1 degrees).
const HALF_BASE_M: f64 = 0.0716;
const CELL_M: f64 = 0.25;
// What the sensor reads facing a wall when the Rover stands in the middle of a cell (measured).
const CENTRED_WALL_M: f64 = 0.118;
const SPIN_RATE: f64 = 7.0;
const SPIN_ACCEL: f64 = 50.0;
const SETTLE_TICKS: u32 = 5;
const DRIVE_SPEED: f64 = 18.0;
const DRIVE_ACCEL: f64 = 40.0;

#[derive(Clone, Copy, PartialEq)]
enum State {
    LookRight,
    LookStraight,
    LookLeft,
    TurnAround,
    Go,
}

struct Rover {
    robot: Robot,
    left: Motor,
    right: Motor,
    eye: DistanceSensor,
    imu: Imu,
    left_target: f64,
    right_target: f64,
    gyro_bias: f64,
    /// Heading from the gyro, radians, positive to the left.
    heading: f64,
    /// The heading we want, in quarter turns.
    quarter: i32,
}

impl Rover {
    /// Let 10 ms pass and add up the gyro. Returns `false` when the mission is over.
    fn tick(&mut self) -> bool {
        if !self.robot.step(10) {
            return false;
        }
        self.heading += (self.imu.gyro()[2] - self.gyro_bias) * STEP_S;
        true
    }

    /// Move the wheels by `dl` and `dr` radians, together.
    fn move_by(&mut self, dl: f64, dr: f64, max_speed: f64, accel: f64) -> bool {
        let dist = dl.abs().max(dr.abs());
        let (l0, r0) = (self.left_target, self.right_target);
        let (mut done, mut speed) = (0.0_f64, 0.0_f64);
        while done < dist {
            let slow = (2.0 * accel * (dist - done)).sqrt();
            speed += accel * STEP_S;
            speed = speed.min(max_speed).min(slow).max(0.5);
            done = (done + speed * STEP_S).min(dist);
            self.left_target = l0 + dl * done / dist;
            self.right_target = r0 + dr * done / dist;
            self.left.position(self.left_target, 40.0);
            self.right.position(self.right_target, 40.0);
            if !self.tick() {
                return false;
            }
        }
        for _ in 0..SETTLE_TICKS {
            if !self.tick() {
                return false;
            }
            if self.left.done() && self.right.done() {
                break;
            }
        }
        true
    }

    /// Spin on the spot until the gyro's heading reaches `target` (radians). The turning rate
    /// follows a trapezoid: it speeds up, and slows down in time to stop at the target.
    fn spin_to(&mut self, target: f64) -> bool {
        let mut w = 0.0_f64; // commanded turning rate, rad/s, positive to the left
        for _ in 0..150 {
            let error = target - self.heading;
            let stop_rate = (2.0 * SPIN_ACCEL * error.abs()).sqrt();
            let mut want = stop_rate.min(SPIN_RATE).max(0.5);
            if error < 0.0 {
                want = -want;
            }
            let step = SPIN_ACCEL * STEP_S;
            w += (want - w).clamp(-step, step);
            self.left.speed(-w * HALF_BASE_M / WHEEL_RADIUS_M);
            self.right.speed(w * HALF_BASE_M / WHEEL_RADIUS_M);
            if !self.tick() {
                return false;
            }
            if (target - self.heading).abs() < 0.008 && w.abs() < 1.0 {
                break;
            }
        }
        self.left.brake();
        self.right.brake();
        self.left_target = self.left.angle();
        self.right_target = self.right.angle();
        true
    }

    /// Turn by a quarter turn (+1 left, -1 right), as the gyro measures it.
    fn turn_quarter(&mut self, dir: i32) -> bool {
        self.quarter += dir;
        self.spin_to(f64::from(self.quarter) * PI / 2.0)
    }

    fn forward_one_cell(&mut self) -> bool {
        let a = CELL_M / WHEEL_RADIUS_M;
        self.move_by(a, a, DRIVE_SPEED, DRIVE_ACCEL)
    }

    /// Look straight ahead. Waits for fresh readings (the sensor updates at 20 Hz and reports
    /// what it saw 30 ms ago), then does two things with them:
    ///  - A reading under 0.2 m is a wall in the way: `blocked`.
    ///  - Walls (or the end of a corridor) are 0.118 m away when the Rover is in the middle of a
    ///    cell, and one more cell (0.25 m) farther for every free cell in between. So the
    ///    reading, taken modulo one cell, tells how far the Rover is from the middle of its cell,
    ///    along the direction it faces. A small error is corrected by driving closer to or
    ///    farther from the wall.
    ///
    /// Returns `(blocked, alive)`; `alive` is `false` if the mission ended.
    fn look(&mut self) -> (bool, bool) {
        let (mut sum, mut first, mut n) = (0.0_f64, -1.0_f64, 0);
        for i in 0..2 {
            for _ in 0..(if i == 0 { 9 } else { 5 }) {
                if !self.tick() {
                    return (true, false);
                }
            }
            let d = self.eye.distance().unwrap_or(-1.0);
            if i == 0 {
                first = d;
            }
            if d > 0.05 && d < 1.2 {
                sum += d;
                n += 1;
            }
        }
        let blocked = first > 0.0 && first < 0.2;
        if n == 0 {
            return (blocked, true);
        }
        let mut offset = (sum / f64::from(n) - CENTRED_WALL_M + 0.5 * CELL_M) % CELL_M;
        if offset < 0.0 {
            offset += CELL_M;
        }
        offset -= 0.5 * CELL_M; // how much farther from the wall than a centred Rover
        if offset.abs() < 0.008 || offset.abs() > 0.06 {
            return (blocked, true);
        }
        let a = offset / WHEEL_RADIUS_M;
        let alive = self.move_by(a, a, 6.0, 15.0);
        (blocked, alive)
    }
}

fn main() {
    let robot = Robot::new();
    let (left, right) = (robot.motor("left"), robot.motor("right"));
    let (eye, imu) = (robot.distance_sensor("eye"), robot.imu());
    let mut r = Rover {
        robot,
        left,
        right,
        eye,
        imu,
        left_target: 0.0,
        right_target: 0.0,
        gyro_bias: 0.0,
        heading: 0.0,
        quarter: 0,
    };

    // The gyro's bias: the average reading while standing still.
    let mut sum = 0.0;
    for _ in 0..40 {
        if !r.robot.step(10) {
            return;
        }
        sum += r.imu.gyro()[2];
    }
    r.gyro_bias = sum / 40.0;

    let mut alive = r.turn_quarter(-1); // the Rover starts facing straight; look right first
    let mut state = State::LookRight;
    while alive {
        match state {
            State::LookRight => {
                let (blocked, ok) = r.look();
                alive = ok;
                if alive && !blocked {
                    state = State::Go;
                } else if alive {
                    alive = r.turn_quarter(1); // back to straight
                    state = State::LookStraight;
                }
            }
            State::LookStraight => {
                let (blocked, ok) = r.look();
                alive = ok;
                if alive && !blocked {
                    state = State::Go;
                } else if alive {
                    alive = r.turn_quarter(1);
                    state = State::LookLeft;
                }
            }
            State::LookLeft => {
                let (blocked, ok) = r.look();
                alive = ok;
                if alive && !blocked {
                    state = State::Go;
                } else if alive {
                    state = State::TurnAround;
                }
            }
            State::TurnAround => {
                alive = r.turn_quarter(1);
                state = State::Go;
            }
            State::Go => {
                alive = r.forward_one_cell();
                if alive {
                    alive = r.turn_quarter(-1); // look right again
                }
                state = State::LookRight;
            }
        }
    }
}
