# 0010: M5 compiler choices the spec leaves open

Date: 2026-11-09. Status: accepted (implementation detail, no locked decision changed).

## Context

SPEC.md fixes the compiler's algorithm but not a few details that decide whether a robot built in
the Builder behaves the way a teenager expects. Each choice below was forced by a test.

## Choices

1. **Joint axes point in the positive dominant direction.** A motor's rotor and housing are two
   sub-bodies; which one is the child depends on the spanning tree. Mirrored motors (left and
   right wheel) therefore got opposite axes and drove backward on equal voltage. The compiler now
   flips each hinge axis so its dominant component is positive and negates it again when the
   motor housing is the child body. Positive volts turn the output the same way regardless of how
   the motor is mounted. A test (`starters.rs`) and V12 (straight line) cover it.
2. **Ids are assigned during emission.** `DeviceMap` and `BodyMap` carry MuJoCo ids computed in
   the same depth-first pre-order the MJCF is written in. A test checks them against MuJoCo's
   name lookup, so the sim and the renderer never search by name at run time.
3. **Deterministic text.** Parts are ordered by uid, numbers are rounded to 1e-9, nothing iterates
   a hash map. Shuffling the part order gives identical MJCF (`tests/shuffle.rs`).
4. **Hardware numbers live in `assets/physics/hardware.ron`.** Solver settings, motor presets,
   the friction table and the joint table are data, all still "placeholder: calibrate" and listed
   in `docs/calibration.md`.
5. **The sandbox lives in `axl-app` and `axl-sim`.** The sim thread owns MuJoCo, steps at 1 ms
   paced against the wall clock (capped at 50 ms of catch-up) and publishes body poses through a
   triple buffer at up to 250 Hz. Commands go the other way over a crossbeam channel. `axl-render`
   does not depend on `axl-sim`: the app copies each new snapshot into the renderer's
   `SnapshotBuffer`.
