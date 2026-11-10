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

## After M6

`axl-hw` (no Bevy) holds the hardware models: `Hardware::new(&DeviceMap, &HwParams, HwOptions, &Physics)` builds a battery, a `Motor` per motor or servo (command mode, PI loops, `Encoder`), and the sensors (`Ultrasonic`, `TimeOfFlight`, `ColorSensor`, `TouchSensor`, `Imu`) from the compiler's device map. The sim loop calls `pre_step` (battery voltage, then motor commands to `ctrl`), `Physics::step`, `post_step` (encoders, currents, charge, then any sensor sample that is due, after a `forward()`). Each sensor samples on a whole-millisecond clock into a fixed 8-entry `DelayQueue`; a read at time t returns the newest sample at or before t minus the delay. Noise comes from per-device `ChaCha8Rng` streams seeded by (run seed, port letter, stream). Devices have handles (their index in `Hardware::devices()`) with a `DeviceKind` and a `Slot`, which the M7 host uses for wrong-device errors. `axl-hw::rig` builds small MJCF rigs for tests and the accuracy suite. Choices the spec leaves open: decision 0012.

## After M7

`axl-host` (no Bevy, no MuJoCo) loads a learner's `.wasm` with wasmtime and exposes the 29-function
ABI (module `axl`) over a `RobotApi` trait; its `toolchain` module finds wasi-sdk and the Rust
target, builds a project (clang or cargo child process, diagnostics parsed to file, line and
column) and copies templates into `<data dir>/projects/<mission>/<c|rust>/`. `axl-sim`'s
`SimRobot` implements `RobotApi` on the sim thread: `axl_step(ms)` runs 1 ms ticks paced against the
wall clock, and `Sandbox::spawn_program` streams `SimEvent`s (log lines, plots, pose marks, lines,
the end of the program). `axl-code` is the Code screen's logic without a window (API reference
parsed from `sdk/c/axl.h`, highlighting, file watching, build sessions); `axl-app::code_ui` is
only egui. The SDKs are `sdk/c/axl.h` (authoritative) and `sdk/rust/axl-robot` (mirrors it).
Try it: `axl --code m01 --code-run`.

## After M8

The instruments sit on both sides of the sim-thread boundary. On the sim thread (`axl-sim`),
`Command` gained `Step`, `Rewind`, `Pick`, `ApplyDragForce` and `ClearDrag`, and `SimEvent` gained
`Restarted`, `Picked` and `Checkpoint`; `SimSnapshot` now carries per-body inspector data, hinge
angle, speed and torque, contacts with forces, the robot's centre of mass, the sensor visuals
(`axl_hw::SensorVisual`) and the built-in plot channels. Rewind restarts the controller and
replays from 0 with drag forces replayed by simulated millisecond; whole-second `Checkpoint`s carry
a joint-position hash and poses for the scrub preview. `axl_sim::plot::PlotStore` keeps the
plotter's series and writes and parses the CSV. In the app, `sim_ui.rs` (`Instruments` resource,
`instruments_tick`, `instruments_input`, `instruments_ui`) drains the events, fills
`axl_render::instruments::LiveView` and `Trails`, handles keys and the mouse and draws the egui
panels; the overlays themselves are gizmos in `axl_render::instruments`, converted with
`axl_to_bevy`. The Code screen and `--sandbox` use the same instruments. Choices: decision 0014.
Try it: `axl --sandbox rover --volts left=2,right=2` (keys 1 to 7, Space, `.`, `[`, `]`, Ctrl+R, drag
with the left button).

## After M9

Missions are RON files in `assets/missions/` (`axl-missions::def`), registered in `library.rs`.
`axl-sim::mission` builds a world and the robot for one mission and seed (`setup`), samples every
10 ms into a `RunTrace` (`Recorder`), runs a controller unpaced (`run_seed`) and grades the traces
of all grading seeds (`grade_mission`, the criteria are in `axl_missions::eval`). The headless
command `axl --headless --mission mNN --controller robot.wasm --seed N [--all-seeds] [--robot
file.axlbot]` prints the result as JSON and exits 0 on a pass, 1 on a fail and 2 on a setup error.
Learning design lives in `axl-missions` without any UI: `learn::fsrs` (FSRS-4.5 cards),
`learn::warmup` (question generators with numbers drawn from a seed, due cards picked per
mission), `progress` (`progress.ron`: stars, failed runs, hints opened, prediction, cards; atomic
write with a `.bak`) and `results` (rows per criterion, metrics, the verdict on the prediction, the
brief as blocks). `axl-app::mission_ui` draws it next to the Code screen: brief, warm-ups,
prediction, hints and the Results screen with a replay of the recorded path; the Code screen hands
a built program over (`CodeScreen::request_submit`) and a thread grades it. Reference solutions:
`missions/solutions/{c,rust}/mNN`, the m12 design `missions/solutions/m12.axlbot`; templates:
`templates/{c,rust}/mNN`. `cargo xtask sdk-check` builds all of them and grades them headless
(templates must not pass, solutions must). Choices: decision 0015.

## After M10

**Screens.** `axl-app::nav` holds `Screen` (Home, Missions, Builder, Code, Simulate, Results,
Settings), the `Nav` resource and the `Action`s screens ask for (`Go`, `OpenMission`,
`OpenSandbox`, `SimulateDesign`, `Watch`). `apply_nav` runs once per frame: it builds or drops the
mission resources (`MissionScreen`, `CodeScreen`, `CodeRobot`, `SandboxScene`, `WorldScene`,
`SandboxSim`), sets `StageKind` and `ViewMode`, and resets the insets. Panels are Bevy systems
gated with `run_if(on(Screen::X))` and, where they need a resource, `resource_exists`.
`Library` (missions, progress, accuracy report) feeds Home, Missions and About. The look-dev,
parts and stress views (`--lookdev`, `--parts`, `--stress-boxes`) keep their own stage and run
with `Nav::active == false`.

**Stage.** `axl-render::stage_swap` has `StageKind` (Backdrop, Builder, Sim). The backdrop scene
and the simulation stage hang under their own roots; `apply_stage_visibility` shows one root.
`rebuild_sim_stage` rebuilds the simulation stage when `SandboxScene` or `WorldScene` changes and
points the camera at it (`SandboxScene::home`). `world_view` draws the mat (a texture, 1 mm
pixels, linear colour), walls and statics, loose objects (moved by body id from the snapshots) and
zones. The picture and the physics use the same numbers: `axl-world::build` returns the shape and
object visuals it wrote into the MJCF.

**Mission world on the stage.** `axl-sim::mission::MissionSetup::spawn_live` starts the sim thread
with the same setup, scenario and seed as a graded run (the first grading seed), paced against
the wall clock. The Sandbox is `axl_missions::library::sandbox()`: the Rover on `open_grid` with
nothing to pass, started through the same path.

**Release.** `cargo xtask package` stages `axl`, the MuJoCo library (rpath `$ORIGIN` or
`@executable_path`, set with `cargo rustc` so dependency builds are reused), `assets/`, `sdk/`,
`templates/`, the guide, the README and the accuracy report (`accuracy-report.md`, read by About),
then packs them with the system `tar` (`.tar.gz`, `.zip`) or `hdiutil` (`.dmg`). `release.yml` runs
it on three OSes for tags `v*`, checks the tag against the Cargo version, runs the accuracy suite
and attaches the packages and the report. The UI scale is a `UiScale` resource in `gui.rs`,
applied with egui's zoom factor and stored as `ui_scale` in `settings.ron`, with the toolchain
paths (`wasi_sdk_path`, `cargo_path`, applied through `axl_host::toolchain::set_overrides`).
