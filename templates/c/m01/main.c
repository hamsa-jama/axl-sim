#include "axl.h"

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    int eye = axl_device_named("eye");
    while (axl_step(10) == AXL_RUNNING) {
        double d = axl_distance(eye);
        double p = (d > 0.0 && d < 0.20) ? 0.0 : 0.5;
        axl_motor_power(left, p);
        axl_motor_power(right, p);
    }
    return 0;
}
