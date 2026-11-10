// Reference solution for m09: a PID line follower that slows down in corners.
// error = how far the colour sensor is from the edge of the line, -1 (over black) to +1 (over
// white). P reacts to the error, D to how fast it changes (it calms the wobble) and I to an
// error that stays on one side. The base power falls when the error is large, so the Rover slows
// into corners and speeds up on straights.
#include "axl.h"

#define EDGE 40.0
#define DT 0.01
#ifndef KP
#define KP 0.8
#endif
#ifndef KD
#define KD 1.5
#endif
#ifndef KI
#define KI 0.2
#endif
#ifndef FAST
#define FAST 0.40
#endif
#ifndef SLOW
#define SLOW 0.16
#endif

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    int color = axl_device_named("color");
    double previous = 0.0, sum = 0.0;
    while (axl_step(10) == AXL_RUNNING) {
        double error = (axl_color_reflect(color) - EDGE) / EDGE;
        sum += error * DT;
        if (sum > 0.5) sum = 0.5;
        if (sum < -0.5) sum = -0.5;
        double derivative = (error - previous) / DT;
        previous = error;
        double turn = KP * error + KI * sum + KD * 0.01 * derivative;
        double size = error < 0 ? -error : error;
        double base = FAST - (FAST - SLOW) * (size > 1.0 ? 1.0 : size);
        axl_motor_power(left, base + turn);
        axl_motor_power(right, base - turn);
        axl_plot_named("error", error);
    }
    return 0;
}
