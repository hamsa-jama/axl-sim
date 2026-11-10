// Mission 02: Counting Turns. Read the mission brief, then write your program here.
#include "axl.h"

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    while (axl_step(10) == AXL_RUNNING) {
        axl_motor_power(left, 0.0);
        axl_motor_power(right, 0.0);
    }
    return 0;
}
