// Reference solution for m06: average 20 readings (one every 50 ms) and report the mean.
#include "axl.h"

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int eye = axl_device_named("eye");
    double sum = 0.0;
    int n = 0;
    axl_step(100); // the first reading arrives after the sensor's delay
    for (int i = 0; i < 20; i++) {
        axl_step(50);
        double d = axl_distance(eye);
        if (d > 0.0) {
            sum += d;
            n++;
        }
    }
    if (n > 0) {
        axl_report_named("distance", sum / n);
    }
    while (axl_step(50) == AXL_RUNNING) {
    }
    return 0;
}
