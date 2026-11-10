// Reference solution for m05: the time-of-flight sensor sees the wall even when it is turned 50
// degrees (the ultrasonic one does not), so drive on its reading. The speed is proportional to how
// far the Rover still has to go, which brings it to a gentle stop.
#include "axl.h"

#define WHEEL_RADIUS_M 0.028
// Measured: with the front of the wheels against a wall this sensor reads 50 mm.
#define SENSOR_TO_FRONT_M 0.050
#define WANTED_GAP_M 0.100

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    int tof = axl_device_named("tof");
    while (axl_step(10) == AXL_RUNNING) {
        double d = axl_distance(tof);
        double speed = 0.3; // m/s while nothing is in range yet
        if (d > 0.0) {
            double error = d - (WANTED_GAP_M + SENSOR_TO_FRONT_M);
            speed = 1.2 * error;
            if (speed > 0.45) speed = 0.45;
            if (speed < -0.1) speed = -0.1;
            // Very slow targets do not move the wheels at all, so creep a little faster.
            if (speed > 0.0 && speed < 0.03) speed = 0.03;
            if (error < 0.0015 && error > -0.0015) speed = 0.0;
        }
        axl_motor_speed(left, speed / WHEEL_RADIUS_M);
        axl_motor_speed(right, speed / WHEEL_RADIUS_M);
    }
    return 0;
}
