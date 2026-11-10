// Mission 06: Filter the Noise. Read the mission brief, then write your program here.
#include "axl.h"

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int eye = axl_device_named("eye");
    while (axl_step(10) == AXL_RUNNING) {
        (void)eye; // read this sensor
    }
    return 0;
}
