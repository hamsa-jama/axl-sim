# 0008: Environment light findings and the new studio map

## Findings
- `--disable env` only removes the camera's `GeneratedEnvironmentMapLight` and adds a flat
  `GlobalAmbientLight`. M3 added no real studio lights or geometry: the "softboxes" were painted into
  the procedural cubemap in `lighting.rs`, so everything is already baked into the env map.
- Bevy 0.20.1's generator makes a specular map at the source size with a full mip chain (9 mips at
  256 per face) and a 32 per face diffuse map. Both are `Rgba16Float`; the generator hard-codes the
  format, so `Rg11b10Ufloat` would need a custom CPU-baked `EnvironmentMapLight`. Not done.
- `freeze_environment` (diagnostics.rs) removes the generator after about 6 steady frames
  ("filtered once and frozen" in the log), so nothing is re-extracted or re-uploaded per frame.
  The remaining cost is per-fragment sampling of the cube maps.
- The regression in per-pass perf lines: the print loop filtered `!p.contains('/')`, but every Bevy
  pass path starts with `render/`. Fixed with `is_nested_pass`, `gpu_top_level` and
  `gpu_summary_lines` (unit-tested).

## New parameters
- `environment.cube_size` (new, default 256) in `assets/render/lighting.ron`; the diffuse map stays 32.
- `environment.intensity`: 130 -> 800. The room is now dim (0.25 to 0.6 relative) and the
  dome is 1.2 instead of 60, so the scale was raised to keep the diffuse level.
- Ring panel removed. 6 softboxes (overhead keys, tall side panels, rim, front strip) plus a ring of
  8 mid-elevation windows, each 10 to 36 relative against a room of about 0.3 (30 to 100 times), edge
  softness 0.0015.

## Not achieved
Crisp, hard-edged softbox reflections do not show on the red brick at High
(`docs/screenshots/env-fix-*.png`). At ABS roughness 0.28 (locked in the spec) Bevy's prefiltered
specular mips blur small panels away; panels large enough to survive the blur wash the scene out. A
real fix needs a custom prefiltered map baked on the CPU, with the sharp mip kept for the clearcoat
(roughness 0.15). Proposed as follow-up. The saturation of the brick is closer to the old look than the
first attempts; judge it on the laptop.
