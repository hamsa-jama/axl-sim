// Reference solution for m08: follow the left edge of the line with a P controller. The colour
// sensor reads about 1 over the black line and about 78 over the white mat, so the edge is at about
// 40. Over white (too far left) the Rover steers right, over black it steers left.
#include "axl.h"

#define EDGE 40.0
#ifndef BASE_POWER
#define BASE_POWER 0.22
#endif
#ifndef GAIN
#define GAIN 0.35
#endif

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    int color = axl_device_named("color");
    while (axl_step(10) == AXL_RUNNING) {
        double error = (axl_color_reflect(color) - EDGE) / EDGE; // -1 (black) to +1 (white)
        double turn = GAIN * error;
        axl_motor_power(left, BASE_POWER + turn);
        axl_motor_power(right, BASE_POWER - turn);
    }
    return 0;
}
