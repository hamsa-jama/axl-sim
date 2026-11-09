# Architecture

See the "Architecture" section of `SPEC.md` for the thread model and message types. This file tracks what the code actually does; update it with each milestone.

## Crates today (after M0)

All crates exist as skeletons. Only `axl-app` has code: a clap command line, a headless mode that prints the version, and an optional Bevy window with an egui status bar.

| Crate | State |
| --- | --- |
| `axl-app` | M0: CLI, headless `--version`, empty window |
| `axl-render`, `axl-parts`, `axl-build`, `axl-compile`, `axl-mujoco`, `axl-hw`, `axl-host`, `axl-sim`, `axl-world`, `axl-missions`, `axl-validate` | Empty skeletons |
| `xtask` | `fetch-mujoco`, `mujoco-env`, `validate` |

## After M1

- `axl-mujoco`: `Physics` (load, step, joints, bodies, sensors, rays with normals, contacts with forces, applied forces, save/restore state, warnings). No `unsafe` yet. `step()` refreshes poses after `mj_step`; contacts, sensors and velocities describe the start of the step until `forward()` is called.
- `axl-validate`: harness (`Row`, `TestResult`, Markdown and JSON reports), test scenes using the spec's fixed MJCF header, tests V01 to V06 and V15.
- Measured on the spec's default `solref="0.004 1"`: V04 creep 0.04 mm in 2 s at 25.5 degrees (limit 2 mm), so no solver retuning was needed yet.

## After M2

- `axl-render` (the only crate besides `axl-app` that may use Bevy) now holds:
  - `frames`: the single AXL-to-Bevy conversion (a root entity rotated -90 degrees about X).
  - `grid` / `grid_math` / `shaders/grid_floor.wesl`: the 200 m grid floor. Lines are drawn in the shader as printed ink (they darken the base colour before lighting), with fwidth anti-aliasing, a 6-pixel spacing fade, a distance fade, red and green world axes, stud-stud and metric units, stud dots under the cursor and a measure tool. The mesh UVs are the floor position in metres, so the shader needs no frame conversion.
  - `lighting`: `assets/render/lighting.ron` and the procedural studio environment (decision 0005).
  - `camera_rig` / `camera_input`: the view camera on Bevy's pan-orbit controller, with orbit, follow and top-down modes, the spec's mouse mapping, exposure, and `apply_quality`, which turns a preset into camera components and shadow settings.
  - `quality`: the four presets and the benchmark.
  - `materials_data`, `placeholder`, `scene`: colours, per-part variation, the micro-surface normal map, placeholder shapes, and the look-dev and stress scenes.
  - `sync`: `SnapshotBuffer` and `SimBody`, which move entities to poses interpolated between the two newest snapshots (the sim thread will publish them from M5/M6).
  - `stage`: `StagePlugin`, which wires all of the above, plus the screenshot runner, the benchmark and the frame-time recorder.
- `axl-app`: the egui control bar (quality, view mode, units, grid opacity, measure tool, camera mode), the corner XYZ gizmo, and `settings.ron` in the user data folder (atomic writes, `.bak`).
