// Reference solution for m01: drive 0.936 m with the built-in position loop, then stay put.
#include "axl.h"

#define WHEEL_RADIUS_M 0.028
#define DISTANCE_M 0.936

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    double target = DISTANCE_M / WHEEL_RADIUS_M;
    axl_motor_position(left, target, 8.0);
    axl_motor_position(right, target, 8.0);
    while (axl_step(10) == AXL_RUNNING) {
    }
    return 0;
}
