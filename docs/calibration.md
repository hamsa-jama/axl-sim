# Calibration list

Every placeholder physical value in AXL is listed here with how to measure it on real hardware. Calibration is owner work and is not part of version 1 acceptance.

The list is filled in as `assets/physics/*.ron` appears (milestones M3 and M6).

| Value | File | Default | How to measure |
| --- | --- | --- | --- |
| Quality benchmark cut-offs (`ultra_max_ms`, `high_max_ms`, `medium_max_ms`) | `assets/render/quality.ron` | 6, 12 and 20 ms average frame time over 3 s at High | Not a physical value. Run `axl --stress-boxes 200 --quality high --perf-seconds 20` on reference machines (see `docs/perf.md`) and set the cut-offs so each class of GPU lands on the preset that meets its target. |
| Motor `v_nom`, `v_max`, `omega0`, `stall_torque`, `resistance`, `armature`, `friction_loss` (medium, large, servo) | `assets/physics/hardware.ron` | 7.4 V, 8.4 V, 26.2/18.3/6.2 rad/s, 0.15/0.40/0.25 N m, 6/4/5 ohm, 1.5e-4/3.0e-4/1.0e-4 kg m2, 0.004/0.008/0.006 N m | No-load speed with a tachometer at nominal voltage; stall torque with a lever arm and a scale; winding resistance with a multimeter at a blocked shaft; armature from the spin-down time with the shaft free; gearbox friction from the voltage at which the shaft starts to turn. |
| Sliding friction of ABS (0.35), rubber (1.0, torsional 0.005), steel (0.2), floor mat (0.5), walls (0.4) | `assets/physics/hardware.ron` | as listed | The angle at which a part starts to slide on an inclined sample of the surface (mu = tan of that angle). |
| Joint damping and friction loss of smooth pin, friction pin, axle in a round hole, gear mesh, caster ball | `assets/physics/hardware.ron` | damping 1e-5 (caster ball 1e-6) N m s/rad; friction loss 0, 0.002, 0.0005, 0.0005 N m | Hang a known torque on the joint and note the smallest value that turns it (friction loss) and the spin-down rate (damping). |
| Contact softness `solref`, `solimp`; minimum body mass | `assets/physics/hardware.ron` | 0.004 1, 0.95 0.99 0.001; 1 g | Not measured on hardware. Tune so a resting robot sinks less than 0.2 mm (V14). |
| Battery: 8.4/7.4/6.4 V full/nominal/empty, 0.15 ohm, 2.0 Ah, 0.08 A idle, 90% start | `assets/physics/hardware.ron` | as listed | Open-circuit voltage at known charge with a multimeter; internal resistance from the voltage drop between idle and a known load; capacity from a timed discharge; idle draw with an ammeter. |
| Speed loop gains (0.6 V s/rad, 6 V/rad) and position loop gain (8 1/s, a design value, decision 0012) | `assets/physics/hardware.ron` | as listed | Tune on the real motor firmware's step response; not a physical constant. |
| Encoder resolution (360 counts/rev, servo 1024 counts over 300 degrees) | `assets/physics/hardware.ron` | as listed | Count pulses over one turn. |
| Ultrasonic, time-of-flight, colour, touch, gyro and accelerometer: ranges, rates, delays, noise sigma, biases, thresholds | `assets/physics/hardware.ron` | the SPEC "Sensors" table | Range and noise: log 10,000 readings against a target at a measured distance and take the standard deviation. Delay: step the target and time the response. Gyro bias and drift: log a still sensor for 10 minutes. Touch threshold: a kitchen scale. |
| Colour reference values (black, white, red, green, blue, yellow) and the intensity curve (peak 8 mm, width 15 mm) | `assets/physics/hardware.ron` | primaries | Read the real mat's inks and a white card at several heights. |
