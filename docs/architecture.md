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

## After M3

- `axl-parts`: `types` (spec part types), `catalog` (RON loader and the built-in catalog of 75 parts), generators (`trimesh`, `earcut`, `profile`, `extrude`, `lathe`, `block`, `shapes`, `devices`, `generate`) that return plain vertex data and an analytic solid volume per part. Part files under `assets/parts/` are checked against the generator by a test.
- `axl-render`: `parts` (part library: uploads each part mesh once), `browser` (debug part browser view and overlays; the egui side panel is `axl-app/src/browser_ui.rs`), `hdri` (optional `assets/render/studio.hdr`), `scene` (look-dev scene from `assets/scenes/lookdev.ron`). Values that differ from the spec text are in decision 0007.

## After M4

- `axl-build` (no Bevy): `orient` (the 24 orientations as an index, fixed numbering), `model` (Assembly, PlacedPart, DeviceInstance, ConnRef, Connection), `geom` (world connectors, the connector hash grid, connector compatibility, collision shapes and penetration depth), `place` (ghost, snapping, validity, cursor rays), `validate` (the panel's checks and exact messages), `builder` (the command API with 200-step undo; the UI and tests both drive it), `io` (`.axlbot`, atomic writes, autosave timer, `AXL_DATA_DIR`), `starter` (Rover and Test chassis recipes run through the same commands).
- Connections are never stored: `ConnIndex::connections()` derives them from placement. Overlaps between a shaft and the part whose hole it passes through are exempt (collision shapes are solid boxes).
- Snapping: the cursor ray hits the nearest part bounds or the floor, the ghost rests on that face (and is also tried pushed into it in 4 mm steps, because holes lie inside parts), every free compatible connector within 6 mm gives a candidate; ranking is overlap-free first, least push, nearest to the ray, fewer overlaps. `U` aims through the robot to its underside.
- `axl-render::builder_view`: draws the robot, ghost, highlights, snap marker, stud dots and surface measure; handles mouse and keys. `axl-app::builder_ui`: egui panels and autosave. `AXL_BUILDER_PROBE=<file>` (test aid) projects robot-frame points to window pixels so scripted xdotool runs can aim real mouse events.

## After M5

`axl-compile` turns an `Assembly` into a `RobotModel` (rigid merge by union-find, hinge links with
the coaxial rule, spanning tree from the hub, `connect` pairs for loops, gear couplings) and then
into MJCF, with a `DeviceMap` (names to actuators, sensors, sites) and a `BodyMap` (MuJoCo body to
the parts it holds). `axl-sim` owns MuJoCo on its own thread and publishes `SimSnapshot`s through a
triple buffer; the app pumps them into `axl-render`'s `SnapshotBuffer`, and `sim_view` hangs each
part's meshes under a `SimBody` entity using the body map. Try it: `axl --sandbox rover
--volts left=4,right=4`. Constants are in `assets/physics/hardware.ron` (placeholder: calibrate).
