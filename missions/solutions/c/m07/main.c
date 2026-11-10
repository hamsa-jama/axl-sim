// Reference solution for m07: a PI controller on the measured speed. The encoder speed is coarse, so
// it is smoothed first. Power = a guess + a part that grows with the error + a part that adds up the
// error over time (which removes the last little offset).
#include "axl.h"

#define WHEEL_RADIUS_M 0.028
#define GOAL_MS 0.25
#define DT 0.01

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    double speed = 0.0, sum = 0.0;
    const double kp = 1.5, ki = 3.0, base = 0.3;
    while (axl_step(10) == AXL_RUNNING) {
        double v = 0.5 * (axl_motor_rate(left) + axl_motor_rate(right)) * WHEEL_RADIUS_M;
        speed = 0.9 * speed + 0.1 * v;
        double error = GOAL_MS - speed;
        sum += error * DT;
        double power = base + kp * error + ki * sum;
        if (power > 1.0) power = 1.0;
        if (power < 0.0) power = 0.0;
        axl_motor_power(left, power);
        axl_motor_power(right, power);
        axl_plot_named("speed", speed);
    }
    return 0;
}
