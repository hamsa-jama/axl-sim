/*
 * axl.h: the C interface to the AXL robot simulator.
 *
 * This file is the authoritative definition of the interface; the Rust crate `axl-robot`
 * mirrors it. A program is an ordinary C program compiled to wasm32-wasip1. Its `main` loops,
 * calling axl_step(ms) to let time pass, like firmware on a real robot.
 *
 * Rules:
 *  - The first AXL call must be axl_init(AXL_ABI_VERSION).
 *  - Time only passes inside axl_step. A loop that never calls it is stopped after 2 seconds.
 *  - A device handle comes from axl_device_named(). It is -1 when the name is not found.
 *  - A wrong handle or a device of the wrong kind stops the program with a message that names
 *    the function, the device and your source line.
 *  - Units are SI: metres, radians, seconds, newtons, volts, amperes.
 *
 * The doc comments below (the ones that start with a double star) are shown in the API
 * reference of the Code screen.
 */
#ifndef AXL_H
#define AXL_H

#include <stdint.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/** The interface version this header speaks. Pass it to axl_init. */
#define AXL_ABI_VERSION 1

/** What axl_step returns. */
enum {
    /** The mission goes on. */
    AXL_RUNNING = 0,
    /** The mission has ended. Finish up and return from main. */
    AXL_MISSION_OVER = 1
};

/** What kind of device a handle is (axl_device_kind). */
enum {
    /** The controller hub (battery and motion sensors). */
    AXL_KIND_HUB = 0,
    /** A motor or servo. */
    AXL_KIND_MOTOR = 1,
    /** An ultrasonic or time-of-flight distance sensor. */
    AXL_KIND_DISTANCE = 2,
    /** A colour sensor. */
    AXL_KIND_COLOR = 3,
    /** A touch sensor. */
    AXL_KIND_TOUCH = 4
};

/** The names axl_color_name can return. */
enum {
    /** Nothing in range. */
    AXL_COLOR_NONE = 0,
    /** Black. */
    AXL_COLOR_BLACK = 1,
    /** White. */
    AXL_COLOR_WHITE = 2,
    /** Red. */
    AXL_COLOR_RED = 3,
    /** Green. */
    AXL_COLOR_GREEN = 4,
    /** Blue. */
    AXL_COLOR_BLUE = 5,
    /** Yellow. */
    AXL_COLOR_YELLOW = 6
};

#define AXL_IMPORT(n) __attribute__((import_module("axl"), import_name(n)))

/* ---- Lifecycle ------------------------------------------------------------------------- */

/** Start talking to the simulator. Must be the first AXL call. Pass AXL_ABI_VERSION. Returns 0. */
AXL_IMPORT("init") int32_t axl_init(int32_t abi);

/** Let `ms` milliseconds pass (1 to 1000). Returns AXL_RUNNING, or AXL_MISSION_OVER when the mission has ended. */
AXL_IMPORT("step") int32_t axl_step(int32_t ms);

/** Simulated time in seconds since the start of the run. */
AXL_IMPORT("time") double axl_time(void);

/* ---- Devices --------------------------------------------------------------------------- */

/** Look up a device by the name you gave it in the builder. Returns its handle, or -1 if there is none. Prefer axl_device_named. */
AXL_IMPORT("device") int32_t axl_device(const char *name, int32_t len);

/** The kind of a device (AXL_KIND_*), or -1 for a handle that does not exist. */
AXL_IMPORT("device_kind") int32_t axl_device_kind(int32_t dev);

/* ---- Motor commands -------------------------------------------------------------------- */

/** Drive a motor with power p from -1 to 1, like a PWM duty cycle: the voltage is p times the battery voltage. */
AXL_IMPORT("motor_power") void axl_motor_power(int32_t dev, double p);

/** Hold a speed in rad/s with the motor's built-in PI loop (it reads the encoder). */
AXL_IMPORT("motor_speed") void axl_motor_speed(int32_t dev, double rad_s);

/** Turn to an angle in radians (as the encoder counts it), never faster than max_rad_s. Check axl_motor_done. A servo stays within plus or minus 90 degrees. */
AXL_IMPORT("motor_position") void axl_motor_position(int32_t dev, double rad, double max_rad_s);

/** Brake: zero voltage, so back-EMF slows the motor down. */
AXL_IMPORT("motor_brake") void axl_motor_brake(int32_t dev);

/** Coast: the motor spins freely with about zero current. */
AXL_IMPORT("motor_coast") void axl_motor_coast(int32_t dev);

/** Make the motor's present angle the new zero of axl_motor_angle. */
AXL_IMPORT("motor_reset_angle") void axl_motor_reset_angle(int32_t dev);

/* ---- Motor readings -------------------------------------------------------------------- */

/** Encoder angle in radians. The encoder counts whole degrees (360 per turn), so this moves in steps of 2 pi / 360. */
AXL_IMPORT("motor_angle") double axl_motor_angle(int32_t dev);

/** Encoder speed in rad/s: the change in counts over the last 10 ms, so it is coarse at low speed. */
AXL_IMPORT("motor_rate") double axl_motor_rate(int32_t dev);

/** Motor current in amperes (negative when the motor is being driven by the wheel). */
AXL_IMPORT("motor_current") double axl_motor_current(int32_t dev);

/** 1 when a position command has arrived (error under 1 degree, nearly stopped), or nothing is pending; else 0. */
AXL_IMPORT("motor_done") int32_t axl_motor_done(int32_t dev);

/* ---- Distance -------------------------------------------------------------------------- */

/** Distance in metres from an ultrasonic or time-of-flight sensor, or -1 when nothing is in range. Ultrasonic sensors miss walls angled more than 40 degrees. */
AXL_IMPORT("distance") double axl_distance(int32_t dev);

/* ---- Colour ---------------------------------------------------------------------------- */

/** Fill rgb3[0..2] with red, green and blue, each 0 to 1023. */
AXL_IMPORT("color_rgb") void axl_color_rgb(int32_t dev, int32_t *rgb3);

/** Reflected light, 0 to 100. */
AXL_IMPORT("color_reflect") int32_t axl_color_reflect(int32_t dev);

/** The nearest colour name (AXL_COLOR_*). */
AXL_IMPORT("color_name") int32_t axl_color_name(int32_t dev);

/* ---- Touch ----------------------------------------------------------------------------- */

/** 1 while the plunger is pressed (force above 1 N for at least 5 ms), else 0. */
AXL_IMPORT("touch_pressed") int32_t axl_touch_pressed(int32_t dev);

/** Force on the plunger in newtons, 0 to 10. */
AXL_IMPORT("touch_force") double axl_touch_force(int32_t dev);

/* ---- Hub ------------------------------------------------------------------------------- */

/** Fill xyz3 with the gyroscope reading in rad/s, in the robot frame. It has noise and a slowly drifting bias. */
AXL_IMPORT("imu_gyro") void axl_imu_gyro(double *xyz3);

/** Fill xyz3 with the accelerometer reading in m/s^2, in the robot frame. At rest it reads gravity (about 9.8 on z). */
AXL_IMPORT("imu_accel") void axl_imu_accel(double *xyz3);

/** The battery voltage right now. It sags when the motors draw current. */
AXL_IMPORT("battery_voltage") double axl_battery_voltage(void);

/* ---- Teaching -------------------------------------------------------------------------- */

/** Send a value to a named channel of the plotter (up to 16 channels). Prefer axl_plot_named. */
AXL_IMPORT("plot") void axl_plot(const char *channel, int32_t len, double value);

/** Tell the simulator where your robot believes it is (metres, radians). It draws a ghost trail. */
AXL_IMPORT("mark_pose") void axl_mark_pose(double x, double y, double yaw);

/** Draw a line in the 3D view, from (x0,y0,z0) to (x1,y1,z1) in metres, with colour 0xRRGGBBAA. */
AXL_IMPORT("draw_line") void axl_draw_line(double x0, double y0, double z0, double x1, double y1, double z1, uint32_t rgba);

/** Remove every line drawn with axl_draw_line. */
AXL_IMPORT("draw_clear") void axl_draw_clear(void);

/* ---- Missions -------------------------------------------------------------------------- */

/** Report a measured value to a mission that asks for one. Prefer axl_report_named. */
AXL_IMPORT("report") void axl_report(const char *key, int32_t len, double value);

/* ---- Helpers that take C strings ------------------------------------------------------- */

/** Look up a device by name. Returns its handle, or -1 if there is none. */
static inline int32_t axl_device_named(const char *name) {
    return axl_device(name, (int32_t)strlen(name));
}

/** Send a value to a named plotter channel. */
static inline void axl_plot_named(const char *channel, double value) {
    axl_plot(channel, (int32_t)strlen(channel), value);
}

/** Report a measured value to the mission under `key`. */
static inline void axl_report_named(const char *key, double value) {
    axl_report(key, (int32_t)strlen(key), value);
}

#ifdef __cplusplus
}
#endif

#endif /* AXL_H */
