// Reference solution for m10: keep the heading with the gyroscope, and dead-reckon the sideways
// position.
// 1. Stand still for 0.4 s and average the gyro's z reading: that is its bias (the pushes only
//    start at 0.5 s).
// 2. Add up (gyro z - bias) x dt: that is the heading, in radians, positive to the left.
// 3. Every step the Rover rolls forward by ds along its heading, which moves it sideways by
//    ds x sin(heading). Add that up too: that is how far to the side it believes it is.
// 4. Steer towards a heading that brings the sideways error back to zero.
#include <math.h>

#include "axl.h"

#define WHEEL_RADIUS_M 0.028
#define GOAL_M 1.5
#define DT 0.01
#ifndef KH
#define KH 40.0
#endif
#ifndef KY
#define KY 3.0
#endif
#ifndef SPEED
#define SPEED 0.25
#endif

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    double gyro[3];

    double bias = 0.0;
    const int samples = 40;
    for (int i = 0; i < samples; i++) {
        if (axl_step(10) != AXL_RUNNING) return 0;
        axl_imu_gyro(gyro);
        bias += gyro[2] / samples;
    }

    double heading = 0.0, side = 0.0, travelled = 0.0;
    double last_l = axl_motor_angle(left), last_r = axl_motor_angle(right);
    while (axl_step(10) == AXL_RUNNING) {
        axl_imu_gyro(gyro);
        heading += (gyro[2] - bias) * DT;
        double l = axl_motor_angle(left), r = axl_motor_angle(right);
        double ds = 0.5 * ((l - last_l) + (r - last_r)) * WHEEL_RADIUS_M;
        last_l = l;
        last_r = r;
        travelled += ds;
        side += ds * sin(heading);

        double speed = SPEED;
        double remaining = GOAL_M - travelled;
        if (remaining < 0.2) speed = SPEED * remaining / 0.2; // slow down gently
        if (remaining < 0.004) speed = 0.0;
        else if (speed < 0.05) speed = 0.05;

        double wanted = -KY * side; // to the left of the line: aim right
        if (wanted > 0.15) wanted = 0.15;
        if (wanted < -0.15) wanted = -0.15;
        // A positive heading error means the Rover points too far left: the left wheel speeds up.
        double steer = KH * (heading - wanted);
        axl_motor_speed(left, speed / WHEEL_RADIUS_M + steer);
        axl_motor_speed(right, speed / WHEEL_RADIUS_M - steer);
        axl_plot_named("heading_deg", heading * 57.29578);
        axl_plot_named("side_mm", side * 1000.0);
    }
    return 0;
}
