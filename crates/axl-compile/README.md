# axl-compile

Assembly to RobotModel to MJCF text, plus the device map and body map.

`compile(catalog, assembly, params, options)` returns the MJCF, a `DeviceMap`, a `BodyMap`, the merged
`RobotModel` and warnings. Output is deterministic: shuffling the part order gives identical text.
Constants come from `assets/physics/hardware.ron` (placeholder: calibrate). See decision 0010.

Part of the AXL workspace. See `SPEC.md` for the design and `docs/architecture.md` for how the crates fit together.
