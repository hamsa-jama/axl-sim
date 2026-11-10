// Reference solution for m07: a PI controller on the measured speed.
// The encoder's own speed reading is coarse (whole degrees every 10 ms), so the speed is measured
// from the angle instead: how far the wheels turned in the last 100 ms.
// Power = a guess + a part that grows with the error + a part that adds up the error over time
// (which removes the last little offset). The sum is clamped so it cannot wind up.
#include "axl.h"

#define WHEEL_RADIUS_M 0.028
#define GOAL_MS 0.25
#define DT 0.01
#define WINDOW 10

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    double history[WINDOW] = {0};
    int n = 0;
    double sum = 0.0;
    const double kp = 1.2, ki = 2.5, base = 0.35;
    while (axl_step(10) == AXL_RUNNING) {
        double angle = 0.5 * (axl_motor_angle(left) + axl_motor_angle(right));
        double old = n >= WINDOW ? history[n % WINDOW] : history[0];
        history[n % WINDOW] = angle;
        n++;
        double speed = n > WINDOW ? (angle - old) * WHEEL_RADIUS_M / (WINDOW * DT) : 0.0;
        double error = GOAL_MS - speed;
        sum += error * DT;
        if (sum > 0.3) sum = 0.3;
        if (sum < -0.3) sum = -0.3;
        double power = base + kp * error + ki * sum;
        if (power > 1.0) power = 1.0;
        if (power < 0.0) power = 0.0;
        axl_motor_power(left, power);
        axl_motor_power(right, power);
        axl_plot_named("speed", speed);
    }
    return 0;
}
