// Reference solution for m12 (with the robot design in missions/solutions/m12.axlbot): drive up
// the ramp and stop on the top.
// The design does the real work. The heavy hub sits over the drive wheels and the caster stands
// well behind them, so the wheels keep their grip when the ramp shifts the weight backwards; the
// plate does not reach out far in front, so its nose clears the foot of the ramp.
// The program ramps the power up gently so the wheels do not spin, counts the distance with the
// wheel angles, and brakes when the top zone is reached (otherwise the robot drives off the edge).
#include "axl.h"

#define WHEEL_RADIUS_M 0.028
// Along the floor (0.25 m) and up the ramp to the middle of the top zone.
#ifndef STOP_AT_M
#define STOP_AT_M 1.30
#endif
#ifndef POWER
#define POWER 0.6
#endif

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    double start = 0.5 * (axl_motor_angle(left) + axl_motor_angle(right));
    double power = 0.0;
    while (axl_step(10) == AXL_RUNNING) {
        double travelled = (0.5 * (axl_motor_angle(left) + axl_motor_angle(right)) - start) * WHEEL_RADIUS_M;
        if (travelled >= STOP_AT_M) {
            axl_motor_brake(left);
            axl_motor_brake(right);
            continue;
        }
        if (power < POWER) power += 0.01;
        axl_motor_power(left, power);
        axl_motor_power(right, power);
    }
    return 0;
}
