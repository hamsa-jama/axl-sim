// Reference solution for m11: keep one hand on the wall (the right-hand rule), as a state machine.
// At every cell the Rover looks right, then straight, then left; it takes the first free side and
// otherwise turns around. It always knows that the cell it came from is free, so it never looks
// back. Looking means turning the Rover and reading the distance sensor: a reading under
// 0.2 m is a wall in the way.
//
// Moves are smooth: a trapezoid speed profile (accelerate, cruise, slow down) feeds a new target to
// the motors' position loops every 10 ms. After every move the heading is corrected with the gyro,
// so that small turning errors do not add up over 40 cells.
#include <math.h>

#include "axl.h"

#define STEP_S 0.01
#define WHEEL_RADIUS_M 0.028
// Half the distance between the drive wheels, as the Rover really turns (measured: a 90 degree
// turn computed with 0.0725 comes out as 91.1 degrees).
#define HALF_BASE_M 0.0716
#define CELL_M 0.25
// What the sensor reads facing a wall when the Rover stands in the middle of a cell (measured).
#define CENTRED_WALL_M 0.118
#define PI 3.14159265358979
#ifndef SPIN_RATE
#define SPIN_RATE 7.0
#endif
#ifndef SPIN_ACCEL
#define SPIN_ACCEL 50.0
#endif
#ifndef SETTLE_TICKS
#define SETTLE_TICKS 5
#endif
#ifndef DRIVE_SPEED
#define DRIVE_SPEED 18.0
#endif
#ifndef DRIVE_ACCEL
#define DRIVE_ACCEL 40.0
#endif

static int left, right, eye;
static double left_target, right_target;
static double gyro_bias, heading; // heading from the gyro, radians, positive to the left
static int quarter;               // the heading we want, in quarter turns

// Let 10 ms pass and add up the gyro. Returns 0 when the mission is over.
static int tick(void) {
    if (axl_step(10) != AXL_RUNNING) return 0;
    double g[3];
    axl_imu_gyro(g);
    heading += (g[2] - gyro_bias) * STEP_S;
    return 1;
}

// Move the wheels by `dl` and `dr` radians, together. Returns 0 if the mission ended.
static int move(double dl, double dr, double max_speed, double accel) {
    double dist = fabs(dl) > fabs(dr) ? fabs(dl) : fabs(dr);
    double l0 = left_target, r0 = right_target;
    double done = 0.0, speed = 0.0;
    while (done < dist) {
        double slow = sqrt(2.0 * accel * (dist - done));
        speed += accel * STEP_S;
        if (speed > max_speed) speed = max_speed;
        if (speed > slow) speed = slow;
        if (speed < 0.5) speed = 0.5;
        done += speed * STEP_S;
        if (done > dist) done = dist;
        left_target = l0 + dl * done / dist;
        right_target = r0 + dr * done / dist;
        axl_motor_position(left, left_target, 40.0);
        axl_motor_position(right, right_target, 40.0);
        if (!tick()) return 0;
    }
    for (int i = 0; i < SETTLE_TICKS; i++) {
        if (!tick()) return 0;
        if (axl_motor_done(left) && axl_motor_done(right)) break;
    }
    return 1;
}

// Spin on the spot until the gyro's heading reaches `target` (radians). The turning rate follows
// a trapezoid: it speeds up, and slows down in time to stop at the target. Returns 0 if the
// mission ended.
static int spin_to(double target) {
    double w = 0.0; // commanded turning rate, rad/s, positive to the left
    for (int i = 0; i < 150; i++) {
        double error = target - heading;
        double stop_rate = sqrt(2.0 * SPIN_ACCEL * fabs(error));
        double want = stop_rate < SPIN_RATE ? stop_rate : SPIN_RATE;
        if (want < 0.5) want = 0.5;
        if (error < 0.0) want = -want;
        double step = SPIN_ACCEL * STEP_S;
        w += want - w > step ? step : (want - w < -step ? -step : want - w);
        axl_motor_speed(left, -w * HALF_BASE_M / WHEEL_RADIUS_M);
        axl_motor_speed(right, w * HALF_BASE_M / WHEEL_RADIUS_M);
        if (!tick()) return 0;
        if (fabs(target - heading) < 0.008 && fabs(w) < 1.0) break;
    }
    axl_motor_brake(left);
    axl_motor_brake(right);
    left_target = axl_motor_angle(left);
    right_target = axl_motor_angle(right);
    return 1;
}

// Turn by a quarter turn (+1 left, -1 right), as the gyro measures it.
static int turn_quarter(int dir) {
    quarter += dir;
    return spin_to(quarter * PI / 2.0);
}

static int forward_one_cell(void) {
    double a = CELL_M / WHEEL_RADIUS_M;
    return move(a, a, DRIVE_SPEED, DRIVE_ACCEL);
}

// Look straight ahead. Waits for fresh readings (the sensor updates at 20 Hz and reports what it
// saw 30 ms ago), then does two things with them:
//  - A reading under 0.2 m is a wall in the way: the function returns 1.
//  - Walls (or the end of a corridor) are 0.118 m away when the Rover is in the middle of a cell,
//    and one more cell (0.25 m) farther for every free cell in between. So the reading, taken
//    modulo one cell, tells how far the Rover is from the middle of its cell, along the direction
//    it faces. A small error is corrected by driving closer to or farther from the wall.
// *alive becomes 0 if the mission ended.
static int look(int *alive) {
    double sum = 0.0, first = -1.0;
    int n = 0;
    for (int i = 0; i < 2; i++) {
        for (int k = 0; k < (i == 0 ? 9 : 5); k++) {
            if (!tick()) {
                *alive = 0;
                return 1;
            }
        }
        double d = axl_distance(eye);
        if (i == 0) first = d;
        if (d > 0.05 && d < 1.2) {
            sum += d;
            n++;
        }
    }
    int blocked = first > 0.0 && first < 0.2;
    if (n == 0) return blocked;
    double offset = fmod(sum / n - CENTRED_WALL_M + 0.5 * CELL_M, CELL_M);
    if (offset < 0.0) offset += CELL_M;
    offset -= 0.5 * CELL_M; // how much farther from the wall than a centred Rover, -0.125 to 0.125
    if (fabs(offset) < 0.008 || fabs(offset) > 0.06) return blocked;
    double a = offset / WHEEL_RADIUS_M;
    *alive = move(a, a, 6.0, 15.0);
    return blocked;
}

typedef enum { LOOK_RIGHT, LOOK_STRAIGHT, LOOK_LEFT, TURN_AROUND, GO } State;

int main(void) {
    axl_init(AXL_ABI_VERSION);
    left = axl_device_named("left");
    right = axl_device_named("right");
    eye = axl_device_named("eye");

    // The gyro's bias: the average reading while standing still.
    double sum = 0.0;
    for (int i = 0; i < 40; i++) {
        if (axl_step(10) != AXL_RUNNING) return 0;
        double g[3];
        axl_imu_gyro(g);
        sum += g[2];
    }
    gyro_bias = sum / 40.0;

    int alive = turn_quarter(-1); // the Rover starts facing straight; look right first
    State state = LOOK_RIGHT;
    while (alive) {
        switch (state) {
        case LOOK_RIGHT: {
            int blocked = look(&alive);
            if (alive && !blocked) {
                state = GO;
            } else if (alive) {
                alive = turn_quarter(+1); // back to straight
                state = LOOK_STRAIGHT;
            }
            break;
        }
        case LOOK_STRAIGHT: {
            int blocked = look(&alive);
            if (alive && !blocked) {
                state = GO;
            } else if (alive) {
                alive = turn_quarter(+1);
                state = LOOK_LEFT;
            }
            break;
        }
        case LOOK_LEFT:
            if (!look(&alive)) {
                state = GO;
            } else if (alive) {
                state = TURN_AROUND;
            }
            break;
        case TURN_AROUND:
            alive = turn_quarter(+1);
            state = GO;
            break;
        case GO:
            alive = forward_one_cell();
            if (alive) alive = turn_quarter(-1); // look right again
            state = LOOK_RIGHT;
            break;
        }
    }
    return 0;
}
