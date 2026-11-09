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
