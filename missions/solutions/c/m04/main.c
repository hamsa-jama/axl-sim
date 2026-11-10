// Reference solution for m04: four times "drive 0.5 m, turn 90 degrees left". The program also keeps
// a believed pose from the commanded moves and marks it so AXL can draw it.
// A smooth move for both wheels: a trapezoid speed profile (accelerate, cruise, slow down) feeds a
// new target to the motors' position loops every 10 ms. Starting and stopping gently keeps the tyres
// from slipping and the Rover from tipping, so the wheel angles tell the truth about the distance.
#include <math.h>

#include "axl.h"

#define STEP_S 0.01

static int left, right;
static double left_target, right_target;

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
        if (speed < 0.3) speed = 0.3;
        done += speed * STEP_S;
        if (done > dist) done = dist;
        left_target = l0 + dl * done / dist;
        right_target = r0 + dr * done / dist;
        axl_motor_position(left, left_target, 30.0);
        axl_motor_position(right, right_target, 30.0);
        if (axl_step(10) != AXL_RUNNING) return 0;
    }
    // Settle: wait for both motors to arrive.
    for (int i = 0; i < 100; i++) {
        if (axl_step(10) != AXL_RUNNING) return 0;
        if (axl_motor_done(left) && axl_motor_done(right)) break;
    }
    return 1;
}

#define WHEEL_RADIUS_M 0.028
#define HALF_BASE_M 0.0725
#define PI 3.14159265358979

static double bx, by, byaw;

static int drive(double metres) {
    double a = metres / WHEEL_RADIUS_M;
    bx += metres * cos(byaw);
    by += metres * sin(byaw);
    int ok = move(a, a, 12.0, 15.0);
    axl_mark_pose(bx, by, byaw);
    return ok;
}

static int turn_left(double angle) {
    double a = HALF_BASE_M * angle / WHEEL_RADIUS_M;
    byaw += angle;
    int ok = move(-a, a, 6.0, 10.0);
    axl_mark_pose(bx, by, byaw);
    return ok;
}

int main(void) {
    axl_init(AXL_ABI_VERSION);
    left = axl_device_named("left");
    right = axl_device_named("right");
    for (int side = 0; side < 4; side++) {
        if (!drive(0.5) || !turn_left(PI / 2.0)) {
            return 0;
        }
    }
    while (axl_step(10) == AXL_RUNNING) {
    }
    return 0;
}
