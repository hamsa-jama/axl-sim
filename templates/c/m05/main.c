// Mission 05: Mind the Wall. Read the mission brief, then write your program here.
#include "axl.h"

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    int tof = axl_device_named("tof");
    while (axl_step(10) == AXL_RUNNING) {
        axl_motor_power(left, 0.0);
        axl_motor_power(right, 0.0);
        (void)tof; // read this sensor
    }
    return 0;
}
