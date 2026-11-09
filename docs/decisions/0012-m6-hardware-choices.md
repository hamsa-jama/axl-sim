# 0012: M6 hardware choices the spec leaves open

Date: 2026-11-09. Status: accepted (implementation detail, no locked decision changed).

## Choices

1. **Ultrasonic 40 degree rule is measured against the sensor axis, not each ray.** SPEC: "Returns
   the nearest hit whose surface faces within 40 degrees of the ray, so angled walls are missed",
   and V16 needs -1 for a wall angled 50 degrees. Per ray, the 15 degree ring still meets a 50 degree
   wall at 35 degrees and would read it, so V16 could not pass. The angle is therefore taken between
   the surface normal and the sensor's axis (a beam, not a ray, is what an angled wall deflects).
2. **Ultrasonic returns the nearest valid ray; the time-of-flight sensor returns the median of its
   valid rays.** The spec gives "nearest" only for the ultrasonic. The mean of the five ToF rays
   reads 0.9 mm off the centre distance on a 50 degree wall (V16 allows 1 mm); the median reads
   0.5 mm off, and is robust at edges. Incidence for the ToF is also measured against the sensor axis
   (limit 75 degrees).
3. **Gyro bias drift is a random walk** with a Gaussian step of 1e-5 rad/s per sample. "Drifts by
   1e-5 rad/s per sample" can also be read as a straight ramp, which would add 0.001 rad/s every
   second and swamp the 0.005 rad/s starting bias within seconds. Noise off switches off bias and drift.
4. **Position loop gain** `position_kp = 8 1/s` (angle error to speed target, then clamped to the
   command's max speed and fed to the spec's speed PI). The spec does not give it. It is a control
   design value, in `hardware.ron`, not a physical constant. The servo uses the same loop, holds 0
   until commanded, and limits its target to +-90 degrees (the joint limit is +-90 degrees in the MJCF).
5. **Colour sensor sampling**: the 5 samples are the centre plus 4 points on the 3 mm circle
   (+-x, +-y of the site), each a ray along the sensor axis. Each sample's intensity is
   1 / (1 + ((d - 8 mm) / 15 mm)^2). A sample with nothing in 4 to 30 mm adds 0. The colour name is
   the nearest reference colour (RGB distance) of the average of the samples that hit, without the
   intensity factor, so name does not change with height. Reference colours are plain primaries in
   `hardware.ron` (placeholder: calibrate). Reflectance is the mean of the three noisy channels / 1023.
6. **Sensor timing in whole milliseconds.** Sample times, delays and update periods are integer ms
   counted by `Hardware::post_step`; the first sample is taken one period after the start, so a
   reading is available from period + delay (ultrasonic: 80 ms).
7. **Derived values are refreshed with `forward()` only on ticks where a sensor samples**
   (after `mj_step`, velocities, contacts and sensor data describe the start of the step). `forward()`
   does not change the trajectory (V15 stays identical). `post_step` therefore takes `&mut Physics`
   (rays need it anyway), not `&Physics` as the spec sketch shows.
8. **Defaults**: motors start in Brake, servos hold position 0. A `Volts` command exists in
   `axl-hw` for the sandbox and the accuracy suite; it is not part of the controller ABI.
9. **Reset rebuilds the hardware** from the seed, so a replay (rewind, M8) reproduces every reading,
   including noise and gyro drift.
10. **Floor mat**: `FloorMat { geom, texture }` maps the ray hit (x, y) to a pixel of a world-space
    rectangle; the mission/world builder (M9) supplies it. Other surfaces use the geom's rgba.
11. **Touch**: the pressed state is debounced every millisecond on the noise-free MuJoCo `touch`
    value; the sampled force reading carries the 0.05 N noise and is limited to 0 to 10 N.
