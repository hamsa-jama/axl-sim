# AXL — Robotics Simulator Design & Development Spec

Version 1.0 of the spec · 9 November 2026 · Owner: hamsa-jama · Live copy: https://claude.ai/artifact/TxdKKc52HXSNikyHrXyfL2

## How to use this spec

This document is the single source of truth for building AXL. Build it milestone by milestone, in the order given under Milestones. Do not change a locked decision without the owner's approval.

**Words used in this spec.** "Must" means required. "Should" means expected unless a decision record explains why not. "Calibrate" marks a starting value that has to be tuned against measurements before release.

**Rules for the implementing model**

1. Read the whole spec before writing any code. Then work on one milestone at a time. A milestone is done only when every acceptance check in its row passes.
2. Do not substitute locked decisions (libraries, languages, units, formats). If one blocks you, stop. Write the problem and two options in `docs/decisions/NNNN-title.md`, then ask the owner.
3. Where the spec gives a number, use it exactly. Where it says "calibrate", put the value in a config file with the default given, and add it to `docs/calibration.md`.
4. Never invent physical data such as masses, torques or friction. Every physical constant lives in `assets/physics/*.ron`, with a comment that names its source or says `placeholder: calibrate`.
5. Keep all `unsafe` code inside the `axl-mujoco` crate, and give every `unsafe` block a `// SAFETY:` comment.
6. Every feature ships with tests. A change to physics must keep the accuracy suite passing.
7. Keep `README.md` and `docs/` current as you go: what works, how to build it, and known gaps.
8. Where this spec is unclear, choose the simplest option that passes the acceptance checks, and record the choice in `docs/decisions/`.

**Definition of done for version 1.** Milestones M0 to M10 are accepted. The accuracy suite passes in CI on Windows, macOS and Linux. A new user can install AXL, build a robot from parts, write a controller in C or Rust, and finish the first mission in under 30 minutes.

## Product summary

AXL is a desktop robotics simulator. Teens build robots from interlocking brick parts, program them in C or Rust, and test them in realistic 3D on research-grade physics. The name is pronounced "axle", and the repository is `axl-sim`.

**Audience.** Learners aged 13 to 18 who can already program a little and want to learn real robotics. Teachers and robotics clubs are secondary users.

**Core loop.** Build a robot, then program it, then simulate it, then measure the result, then improve it.

**What makes it different**

- **Real physics:** robots built from bricks become a MuJoCo model, the same physics engine used in robotics research.
- **Realistic hardware:** motors and sensors behave like real hardware, with real limits, noise and delay.
- **Real languages:** controllers are written in C or Rust, the languages used on real robot microcontrollers. Each controller is compiled to a sandboxed WebAssembly module.
- **Visible physics:** the simulator shows what is normally invisible: forces, centre of mass, coordinate frames, and live signal plots.
- **Professional bridge:** every robot exports as a MuJoCo model file that opens in professional tools.

**Success measures for version 1**

| Area | Target |
| --- | --- |
| Physics accuracy | Every check in the accuracy suite passes within its stated tolerance |
| Visual realism | 60 fps at 1920×1080 on the High preset with a mid-range discrete GPU; 30 fps on the Low preset with integrated graphics |
| Learning content | 12 missions, each graded automatically and identically on every run |
| Determinism | The same robot, code and seed produce a bit-identical run on the same machine |
| Onboarding | A new user finishes the first mission in under 30 minutes |

## Locked decisions

AXL is a native desktop app written in Rust, on MuJoCo physics and the Bevy engine. Learner robot programs are written in C or Rust and run as WebAssembly modules. Versions are the latest stable releases as of November 2026. Pin them exactly in `Cargo.lock`, and upgrade only through a decision record.

| Area | Decision | Reason |
| --- | --- | --- |
| Platform | Native desktop: Windows 10+ (x64), macOS 13+ (Apple silicon), Ubuntu 22.04+ (x64). A browser build is Phase 4, not version 1. | Realistic rendering and real compilers |
| Product languages | Rust (edition 2024, stable, 1.95 or newer) for all AXL code. The only C is MuJoCo itself. No Python, JavaScript or TypeScript anywhere, including build tooling. Tooling scripts use the `cargo xtask` pattern. | Owner requirement |
| Learner languages | C (C17) and Rust, both compiled to the `wasm32-wasip1` target | The languages used on real robot controllers |
| Physics | MuJoCo 3.12.0 through the `mujoco-rs` crate, version 6.1.0+mj-3.12.0. Default features only: no viewer, no renderer. | Research-grade contact and joint physics |
| Rendering and app framework | Bevy 0.20 (wgpu backend) | Physically based rendering in pure Rust |
| Tool UI | `bevy_egui` 0.43.1 (egui 0.36), plus the `egui_plot` release built for egui 0.36 | Mature panels, text editing and plotting |
| Controller sandbox | `wasmtime` 50.0.0 with WASI preview 1, fuel off, epoch interruption on | Safe, deterministic and cross-platform |
| Units | SI everywhere inside the code: metres, kilograms, seconds, radians, newtons, newton-metres, volts, amps | Avoids unit bugs |
| Coordinates | Right-handed and Z-up in physics, files and the robot API. Convert to Bevy's Y-up only inside `axl-render`. | MuJoCo and robotics convention |
| Timing | Physics step 1 ms (1000 Hz). Controllers advance in whole milliseconds. Rendering runs at display rate, interpolated between physics states. | Stable contacts and realistic control rates |
| Solver settings | `integrator="implicitfast"`, `cone="elliptic"`, `impratio="10"` | Stable contact and grasping |
| Data formats | RON for parts, robots, worlds, missions and settings. MJCF XML for MuJoCo export. | Readable and diff-friendly |
| Licence | AXL code under MIT OR Apache-2.0. Third-party art assets must be CC0. | Simple redistribution |
| Branding | Generic interlocking-brick parts only. Never use the LEGO name, logo, part numbers or minifigure shapes. | Trademark safety |

Sources: [Bevy news](https://bevy.org/news/), [Bevy 0.20 release notes](https://bevy.org/news/bevy-0-20/), [mujoco-rs](https://docs.rs/crate/mujoco-rs/latest), [wasmtime](https://docs.rs/crate/wasmtime/latest), [bevy\_egui](https://docs.rs/crate/bevy_egui/latest).

## Non-goals for version 1

These are out of scope for version 1. Do not build them, even partially, unless a later phase in Milestones adds them.

- A browser or WebGPU build (Phase 4).
- A ROS 2 bridge or any network connection (Phase 4).
- Breakable brick connections (Phase 3).
- Gear backlash. Gears are an exact ratio with a friction loss.
- Soft bodies, cables, fluids, or flexing plastic.
- Python, JavaScript, block-based programming, or any learner language other than C and Rust.
- Bundled compilers. AXL detects the toolchains already installed and walks the user through installing them.
- Real-time path tracing (Bevy Solari). It may come later as an optional photo mode.
- Reinforcement-learning training loops or GPU-parallel simulation.
- User accounts, cloud saves, telemetry, or analytics.
- Importing outside CAD models as parts.

## Architecture

AXL runs on two main threads. The **app thread** is Bevy: it handles rendering, input, the UI and the builder. The **sim thread** owns MuJoCo, the hardware models and the learner's controller, and steps all three in lockstep. Nothing else touches MuJoCo.

```text
+-------------------------+      Background tasks       +----------------------------+
| App thread (Bevy)       |                             | Sim thread                 |
|                         |   +--------------------+    |                            |
|  [Code editor] ---------+-->| Toolchain task     |----+--> [Controller (wasmtime)] |
|                         |   | source in,.wasm out|    |        learner C or Rust   |
|                         |   +--------------------+    |            | calls axl_step|
|  [Builder] -------------+-->| Compile task       |--->|            v               |
|                         |   | assembly in,       |    |    [Robot API host]        |
|                         |   | MJCF out           |    |  axl_step, devices, limits |
|                         |   +--------------------+    |            | runs N x 1 ms |
|  [Renderer, overlays] <-+---- snapshots, up to 250 Hz-+            v               |
|                         |                             |    [1 ms step loop]        |
|  [UI, time controls] ---+---- commands: pause, speed, +->   hardware models        |
|                         |     rewind, drag            |    MuJoCo step             |
|                         |                             |    mission checks          |
+-------------------------+                             +----------------------------+
```
Only the sim thread touches MuJoCo; the app thread only reads snapshots and sends commands.

The learner's code and the physics share the sim thread and take turns. The app thread only reads snapshots and sends commands.

**Threads and how they talk**

1. **App thread (Bevy).** Rendering, camera, UI, builder and mission screens. It never calls MuJoCo directly.
2. **Sim thread.** One per running simulation. It owns `MjModel`, `MjData`, every hardware model, and the wasmtime `Store` holding the controller. When the controller calls `axl_step(ms)`, the host runs `ms` physics steps of 1 ms each and updates the hardware models after every step. It then paces against the wall clock and returns. With no controller loaded, the sim thread steps on its own in real time.
3. **Compile task.** Runs on Bevy's `AsyncComputeTaskPool` when the user presses Simulate. It turns the robot assembly into MJCF text.
4. **Toolchain task.** A background thread that runs `cargo` or `clang` as a child process and streams the output to the build log.

**Messages between threads**

- **App to sim.** Commands on a `crossbeam-channel`: `Pause`, `Resume`, `StepOnce`, `SetSpeed(f32)`, `Rewind(seconds)`, `Reset`, `Stop`, `ApplyDragForce{body, point, force}`.
- **Sim to app.** A `SimSnapshot` published through a triple buffer every 4 ms (up to 250 Hz). It holds sim time, every body's pose, joint angles, contacts with forces, sensor visual data, new plot samples and new log lines. The app interpolates poses between the two newest snapshots.
- **Speed control.** Pacing targets wall-clock time multiplied by the speed factor (0.1× to 4×). Mission grading and headless runs use "as fast as possible", so they skip pacing.

**Cargo workspace**

| Path | Responsibility | May depend on Bevy |
| --- | --- | --- |
| `crates/axl-app` | The binary: screens, wiring, and the headless command-line mode | Yes |
| `crates/axl-render` | Materials, mesh upload, the grid floor, overlays, camera, quality presets, Y-up conversion | Yes |
| `crates/axl-parts` | Part catalog types, RON loading, connectors, procedural mesh generation as plain vertex data | No |
| `crates/axl-build` | The robot assembly: placed parts, connections, snapping, validation, undo and redo | No |
| `crates/axl-compile` | Assembly to `RobotModel` to MJCF text, plus the device map | No |
| `crates/axl-mujoco` | The only crate allowed `unsafe`. A small safe layer over `mujoco-rs` for loading, stepping, state save and restore, rays, contacts and sensors. | No |
| `crates/axl-hw` | Motor, battery, encoder and sensor models, with seeded random noise | No |
| `crates/axl-host` | The wasmtime host: ABI functions, memory limits, timeouts | No |
| `crates/axl-sim` | The sim thread: lockstep loop, pacing, snapshots, rewind buffer | No |
| `crates/axl-world` | World and object definitions, and arena building | No |
| `crates/axl-missions` | Mission definitions, success evaluators, scoring, progress | No |
| `crates/axl-validate` | The accuracy suite and the report generator | No |
| `sdk/c/` | `axl.h` and its documentation for learners | n/a |
| `sdk/rust/axl-robot/` | The Rust crate learners use | n/a |
| `templates/c/`, `templates/rust/` | Starter projects for each mission | n/a |
| `assets/` | Part, physics, world and mission RON files, CC0 textures, the HDR environment map | n/a |
| `xtask/` | `fetch-mujoco`, `validate`, `package`, `sdk-check` | No |
| `docs/` | Decision records, calibration list, user guide | n/a |

**Rules**

- Only `axl-app` and `axl-render` may depend on Bevy. Every other crate must build and test headless.
- Headless mode must run a mission without a window: `axl --headless --mission m03 --controller path/to/robot.wasm --seed 1`. It prints a JSON result and exits with 0 on pass or 1 on fail. CI and the accuracy suite use this mode.

## Units, frames and brick dimensions

The brick system uses an 8.0 mm stud pitch, a 9.6 mm brick height and a 3.2 mm plate height. Gears use module 1 mm, so standard pairs mesh on the 8 mm hole grid. All code works in SI units, Z-up.

**Units**

| Quantity | Unit in code | Shown in UI |
| --- | --- | --- |
| Length | metre (`f64`) | millimetres or studs (user toggle) |
| Placement in saved files | integer, 0.1 mm per step (`i32`) | studs and plates |
| Mass | kilogram | grams |
| Time | second | seconds |
| Angle | radian | degrees |
| Angular velocity | radian per second | rpm |
| Force, torque | newton, newton-metre | N, N·cm |
| Electrical | volt, amp, ohm | same |

**Frames**

- **World frame.** Right-handed, Z up, with the origin at the centre of the floor and the floor at z = 0. Axis colours: X red, Y green, Z blue.
- **Robot frame.** Robotics convention: origin at the centre of the controller hub, +X forward (the hub's arrow), +Y left, +Z up. Yaw is measured counter-clockwise from world +X.
- **Part frame.** For bricks and plates: origin at the centre of the bottom face, +Z through the studs, +X along the long side. For beams: origin at the centre of the first hole, holes along +X, hole axes along local Y.
- **Orientation in the builder.** Version 1 allows only the 24 axis-aligned orientations, stored as an index from 0 to 23. Joints always start at angle 0. This keeps snapping exact, with no floating-point drift.
- **Rendering conversion.** Generate every mesh in the AXL Z-up frame. In Bevy, put all AXL entities under one root entity whose rotation is −90° about X. That one rotation maps AXL (x, y, z) to Bevy (x, z, −y). No other code converts frames.

**Brick dimensions** (generic interlocking-brick standard)

| Feature | Value |
| --- | --- |
| Stud pitch | 8.0 mm |
| Brick height (without stud) | 9.6 mm |
| Plate height | 3.2 mm |
| Footprint of an n × m part | (8.0 n − 0.2) × (8.0 m − 0.2) mm |
| Stud diameter × height | 4.8 × 1.7 mm (drawn only, never collides) |
| Wall thickness | 1.2 mm |
| Beam cross-section | 7.2 × 7.2 mm |
| Hole diameter and pitch | 4.8 mm, 8.0 mm along the beam |
| Pin | diameter 4.8 mm, length 16 mm |
| Axle | cross section 4.8 mm wide with 1.8 mm arms, length in 8 mm steps |
| Gear module | 1.0 mm, so pitch diameter in mm equals the tooth count |
| Gear centre distance | (teeth₁ + teeth₂) / 2 mm. For example, 8 + 24 teeth = 16 mm = 2 holes. |

## Part catalog

Version 1 ships 75 parts, each defined in one RON file under `assets/parts/`. A part's mesh is generated by code from its parameters, so no 3D artist is needed. Every part declares its mass, collision shapes and connectors.

**Part definition types** (in `axl-parts`)

```rust
pub struct PartDef {
    pub id: PartId,                    // stable snake_case, e.g. "brick_2x4"
    pub name: String,                  // shown in UI, e.g. "Brick 2×4"
    pub category: Category,            // Brick, Plate, Tile, HoleBrick, Beam, Axle, Pin, Gear, Wheel, Caster, Hub, Motor, Sensor
    pub shape: ShapeGen,               // procedural generator and its parameters
    pub bodies: Vec<SubBody>,          // one for simple parts; two for motors (housing, rotor)
    pub internal_joint: Option<InternalJoint>, // motors only: hinge between housing and rotor
    pub material: MaterialKind,        // Abs, AbsTransparent, Rubber, Steel
    pub colors: Vec<ColorId>,          // allowed colours; the first is the default
    pub device: Option<DeviceDef>,     // motors, sensors and hub (see Hardware models)
}
pub struct SubBody {
    pub mass_kg: f64,                  // never zero
    pub mass_source: MassSource,       // Computed { density_kg_m3 } | Measured { note } | Placeholder
    pub collision: Vec<CollisionPrim>, // in the part frame
    pub connectors: Vec<Connector>,
}
pub enum CollisionPrim {
    Box { center: [f64; 3], half: [f64; 3] },
    Cylinder { center: [f64; 3], axis: Axis, radius: f64, half_len: f64 },
    Sphere { center: [f64; 3], radius: f64 },
}
pub struct Connector { pub kind: ConnKind, pub pos: [i32; 3] /* 0.1 mm */, pub axis: Axis /* ±X ±Y ±Z */ }
pub enum ConnKind { Stud, AntiStud, RoundHole, AxleHole, PinShaft { friction: bool }, AxleShaft { len: u8 } }
```

**Connector rules.** Only these pairs can join. The builder refuses every other pair.

| Male | Female | Result |
| --- | --- | --- |
| `Stud` | `AntiStud` | Rigid |
| `PinShaft` | `RoundHole` | Hinge about the hole axis. A friction pin adds joint friction. |
| `AxleShaft` | `RoundHole` | Hinge about the hole axis (the axle spins freely) |
| `AxleShaft` | `AxleHole` | Rigid (the cross locks rotation) |

Axles do not slide along their length in version 1. Their axial position is fixed where they are placed.

**Mass.** Plastic parts compute mass as solid volume times ABS density, 1040 kg/m³. The shape generator returns the volume analytically, including hollow undersides. Motors, sensors, wheels and the hub use measured or placeholder masses from the Hardware models section.

**Collision.** Collision uses simple shapes only: boxes, cylinders and spheres. Studs, tubes and gear teeth never collide. In version 1, parts of the same robot never collide with each other. Robot-to-world and robot-to-object contacts are always on.

**Version 1 part list**

| Family | Parts | Count |
| --- | --- | --- |
| Bricks | 1×1, 1×2, 1×3, 1×4, 1×6, 1×8, 2×2, 2×3, 2×4, 2×6, 2×8 | 11 |
| Plates | 1×1, 1×2, 1×3, 1×4, 1×6, 1×8, 2×2, 2×4, 2×8, 4×8, 6×8, 8×16 | 12 |
| Tiles (smooth top) | 1×1, 1×2, 1×4, 2×2, 2×4 | 5 |
| Bricks with holes | 1×2, 1×4, 1×6, 1×8, 1×12 (holes along the long side) | 5 |
| Beams | straight 3, 5, 7, 9, 11, 13 and 15 holes; L-shape 3×5; frame 5×7 | 9 |
| Axles | lengths 2, 3, 4, 5, 6, 7, 8, 10, 12 | 9 |
| Pins | friction pin, smooth pin, long friction pin (3 units), axle-pin | 4 |
| Spur gears | 8, 12, 16, 20, 24, 36 and 40 teeth | 7 |
| Wheels | diameter 30, 43, 56 and 81 mm, each with a rubber tyre | 4 |
| Caster | ball caster with a 16 mm steel ball | 1 |
| Hub | controller hub with battery, built-in IMU and six ports, A to F | 1 |
| Motors | medium motor, large motor, 180° servo | 3 |
| Sensors | ultrasonic distance, time-of-flight distance, colour, touch | 4 |
| Colour variants | the same parts in 9 colours; each colour/part pair is not a separate part | n/a |

That is 75 parts: 67 shape parts (including the caster) and 8 devices (the hub, three motors and four sensors). Phase 2 adds bevel gears, a worm gear, omni wheels and gripper parts. Phase 3 adds a camera and a 2D lidar.

**Ports.** Every motor and sensor must be plugged into one of the hub's six ports. That limits a robot to six devices, as on real hardware. The builder assigns the next free port automatically, and the user can change it.

## Builder

The builder places parts on an exact grid and derives every connection from geometry. Connections are never stored: placement is the single source of truth, and connections are rebuilt after every edit and on load.

**Assembly data** (in `axl-build`)

```rust
pub struct Assembly { pub name: String, pub parts: Vec<PlacedPart> }
pub struct PlacedPart {
    pub uid: u32,                       // unique within the assembly, never reused
    pub part: PartId,
    pub pos: [i32; 3],                  // 0.1 mm steps, part frame origin in the robot frame
    pub orient: u8,                     // 0..=23, one of the axis-aligned orientations
    pub color: ColorId,
    pub device: Option<DeviceInstance>, // motors, sensors, hub
}
pub struct DeviceInstance { pub name: String /* [a-z][a-z0-9_]{0,23} */, pub port: Option<Port> /* A..F; None only for the hub */ }
pub struct Connection { pub a: ConnRef, pub b: ConnRef, pub kind: JointKind } // derived only
pub enum JointKind { Rigid, Hinge { friction: bool } }
```

**Placing a part**

1. The user picks a part from the palette. A translucent ghost follows the cursor.
2. The ghost snaps when one of its connectors is within 6 mm of a free, compatible connector on the robot, with matching axes. If several candidates qualify, pick the nearest one to the cursor ray, then the one with fewer overlaps.
3. `R` turns the ghost 90° about Z, `T` about X, and `Y` about Y.
4. A click places it, but only when the placement is valid:
   - it makes at least one connection (the first part is exempt), and
   - no collision shape overlaps another by more than 0.1 mm (touching is allowed).
5. After placing, every pair of compatible connectors whose positions coincide to within 0.05 mm becomes a connection, not only the snapped pair.

Use a hash grid keyed by connector position, at 0.1 mm resolution, so connector lookup is constant time.

**Editing.** Click selects and Shift-click adds to the selection. `Delete` removes, `Ctrl+D` duplicates, and `G` picks the selection up to place it again. A colour swatch recolours. Undo and redo (`Ctrl+Z`, `Ctrl+Shift+Z`) use a command stack 200 steps deep.

**Validation.** Checks run after every edit and show in a panel. Errors block Simulate; warnings do not.

| Check | Level | Message to show |
| --- | --- | --- |
| No hub, or more than one hub | Error | "A robot needs exactly one hub." |
| More than six devices | Error | "The hub has six ports. Remove a motor or sensor." |
| Parts not connected to the rest | Error | "N parts are floating. Connect them or delete them." Highlight them in red. |
| Two devices share a name or a port | Error | "Two devices are called left\_motor." |
| Overlapping parts | Error | Highlight both parts. |
| A motor's output connects to nothing | Warning | "Motor B turns nothing." |
| A wheel is not on an axle | Warning | "This wheel can't turn." |

**Starter robots.** Ship two prebuilt assemblies in `assets/robots/`: *Rover* (two medium motors, two 56 mm wheels, a caster, an ultrasonic sensor and a colour sensor) and *Test chassis* (a hub on a plate with no devices). A mission can start from a starter robot and can lock the builder.

## Assembly-to-physics compiler

The compiler turns an assembly into one MJCF document plus a device map. Rigidly connected parts merge into one MuJoCo body, hinges become joints, meshed gears become coupling constraints, and motors become actuators. The same assembly must always produce byte-identical MJCF, so sort everything by part `uid`.

**Outputs** (in `axl-compile`)

- `mjcf: String`: the full model, including the world and objects from the mission.
- `DeviceMap`: each device name mapped to its MuJoCo ids (joint, actuator, site, sensors) and its hardware parameters.
- `BodyMap`: each MuJoCo body mapped to the part `uid`s it holds and each part's transform in the body frame. The renderer uses it to attach part meshes to moving bodies.

**Algorithm**

1. **Nodes.** Make one node per sub-body. Most parts give one node; a motor gives two, its housing and its rotor, joined by its internal hinge.
2. **Rigid merge.** Union-find over every `Rigid` connection. Each resulting set is a *cluster*, which becomes one MuJoCo body.
3. **Hinge links.** Map every `Hinge` connection to the pair of clusters it joins. Drop links inside one cluster. For each pair of clusters:
   - if all links between them share one axis line (directions parallel within 1e-6, lines within 0.05 mm), they become one hinge joint, with friction if any link is a friction pin;
   - otherwise, merge the two clusters, because two non-coaxial hinges lock rotation.
   - Repeat until nothing changes.
4. **Pin clusters.** A cluster made only of pins, hinged to exactly two clusters on the same axis, merges into the neighbour with the lower smallest `uid`. Its other hinge stays. This avoids tiny bodies that destabilise the solver.
5. **Tree and loops.** The root is the cluster holding the hub. Build a spanning tree from the root by breadth-first search, visiting joints in ascending id order. A joint not in the tree closes a loop, for example a four-bar linkage. Express it as two `<connect>` equality constraints, at the anchor point and 8 mm along the axis.
6. **Gears.** Two gears mesh when their axes are parallel, their centre distance equals (teeth₁ + teeth₂) / 2 mm within 0.05 mm, and their faces overlap. Let j₁ and j₂ be the hinge joints carrying each gear. The pair is supported only when both joints have the same parent body. It becomes `<joint joint1="j1" joint2="j2" polycoef="0 r 0 0 0"/>` with r = −teeth₂ / teeth₁. If the parents differ, report the error "Gears must turn on axles held by the same part".
7. **Bodies.** A child body's origin is its joint anchor; the root's origin is the hub centre. Body axes match the robot frame at build time, since every joint starts at 0. The root gets `<freejoint name="root"/>`.
8. **Geoms.** One geom per collision shape, moved into its body frame. Set each geom's `mass`: its sub-body's mass split by shape volume. MuJoCo then computes the inertia (`inertiafromgeom="true"`). Friction comes from the material table in Hardware models.
9. **Collision filtering.** Robot geoms use `contype="1" conaffinity="2" group="1"`. Static world geoms use `contype="2" conaffinity="1" group="0"`. Loose objects use `contype="3" conaffinity="3" group="2"`. So a robot never hits itself, but robots, world and objects all hit each other.
10. **Joints.** `<joint type="hinge">` with `pos`, `axis`, `damping`, `frictionloss` and `armature` taken from the joint and motor tables in Hardware models.
11. **Motors.** Each motor's internal hinge gets a `<general>` actuator that models a DC motor exactly. Here `ctrl` is the applied voltage, `gainprm = τ_stall / V_nom`, and `biasprm = "0 0 −τ_stall/ω_0"`. Set `forcerange = ±τ_stall` and `ctrlrange = ±V_max`. With `implicitfast`, the back-EMF term is integrated implicitly, which keeps it stable at 1 ms.
12. **Sensors.** Add a `<site>` for every sensor at the frame given in its part definition. The hub adds `<gyro>`, `<accelerometer>` and `<framequat>` on site `imu`. Each motor joint adds `<jointpos>` and `<jointvel>`, and each touch sensor adds `<touch>` on its plunger site. Distance and colour sensors are ray casts done in `axl-hw`, not MJCF sensors.
13. **Spawn.** Place the robot at the mission's spawn pose, with its lowest collision point 0.5 mm above the floor.

**Fixed MJCF header**

```xml
<mujoco model="axl">
  <compiler angle="radian" inertiafromgeom="true" autolimits="true"/>
  <option timestep="0.001" integrator="implicitfast" cone="elliptic" impratio="10" gravity="0 0 -9.80665"/>
  <default>
    <geom condim="3" solref="0.004 1" solimp="0.95 0.99 0.001"/>
  </default>
  <!-- worldbody, equality, actuator, sensor follow -->
</mujoco>
```

`solref="0.004 1"` is a starting value: calibrate it so a resting robot sinks less than 0.2 mm into the floor. Tyres override `condim="4"` to add torsional friction.

**Export.** "Export to MuJoCo" writes the same MJCF to a `.xml` file. It must load in desktop MuJoCo 3.12.0 and behave identically, apart from the hardware models, which live in AXL.

## Physics runtime

MuJoCo runs single-threaded on the sim thread with a fixed 1 ms step. Every source of randomness is seeded, so a run can be replayed exactly. Rewind is a fast deterministic replay from the start, not a memory snapshot.

**Getting MuJoCo.** `cargo xtask fetch-mujoco` downloads the official MuJoCo 3.12.0 release archive for the host OS. It checks the archive's SHA-256 against the value pinned in `xtask/src/mujoco_checksums.rs` (copy it from the official release page) and extracts it to `third_party/mujoco/3.12.0/<os>/`. Link it as `mujoco-rs` documents for a shared library, and record the exact environment variables in `docs/build.md`. Packaging puts the library next to the executable:

- Windows: `mujoco.dll` beside `axl.exe`.
- Linux: `libmujoco.so.3.12.0`, with rpath `$ORIGIN`.
- macOS: `mujoco.framework` in `AXL.app/Contents/Frameworks`, with rpath `@executable_path/../Frameworks`.

**Safe API** (in `axl-mujoco`, the only crate allowed `unsafe`)

```rust
pub struct Physics { /* MjModel + MjData */ }
impl Physics {
    pub fn from_mjcf(xml: &str) -> Result<Self, PhysicsError>;
    pub fn step(&mut self);                                   // exactly one mj_step
    pub fn time(&self) -> f64;
    pub fn set_ctrl(&mut self, a: ActuatorId, volts: f64);
    pub fn joint_pos(&self, j: JointId) -> f64;
    pub fn joint_vel(&self, j: JointId) -> f64;
    pub fn body_pose(&self, b: BodyId) -> Pose;               // world position + quaternion
    pub fn sensor(&self, s: SensorId) -> &[f64];
    pub fn ray(&self, from: [f64; 3], dir: [f64; 3], groups: GroupMask) -> Option<RayHit>; // mj_ray
    pub fn contacts(&self) -> Vec<ContactInfo>;               // position, normal, force via mj_contactForce
    pub fn apply_force(&mut self, b: BodyId, point: [f64; 3], force: [f64; 3]); // xfrc_applied
    pub fn warnings(&self) -> PhysicsWarnings;                // read the mjData warning counters
}
```

Rays pass `GroupMask` = groups 0 and 2 only, so a robot's sensors never see the robot itself (group 1).

**The step loop** (in `axl-sim`), run for each 1 ms tick:

1. `hw.pre_step(&mut physics, t)`: turn motor commands into voltages, apply battery sag, run servo loops, set `ctrl`.
2. `physics.step()`.
3. `hw.post_step(&physics, t)`: update encoders, currents and sensor sample-and-hold buffers, and add noise.
4. `mission.observe(&physics, &hw, t)`: update success and failure checks.
5. Publish a `SimSnapshot` every 4 ms (250 Hz cap).
6. Check `physics.warnings()`. On any NaN or bad-acceleration warning, stop and show: "The simulation became unstable. This usually means a part is very light or a joint is overloaded."

**Determinism rules**

- No MuJoCo multithreading and no thread pool in the sim thread.
- All noise comes from `rand_chacha::ChaCha8Rng`, seeded from the run seed and the device's port letter.
- In sim crates, iterate only over `Vec` or `BTreeMap`, never `HashMap`.
- Inside the controller, WASI `clock_time_get` returns sim time and `random_get` returns seeded bytes. Controllers never see wall-clock time.
- Promise bit-identical runs only on the same build and machine. Mission grading uses tolerances, so results match across operating systems.

**Rewind and replay.** Because runs are deterministic, rewinding to time *t* means resetting and re-running from 0 to *t* with pacing off, controller included. The scrub bar shows progress while it replays. While replaying, keep a checkpoint of physics and hardware state every 1 s, so the scrub bar can preview poses without re-running.

**Mouse drag.** Dragging a body applies a spring force through `xfrc_applied`: stiffness 20 N/m times the robot's mass in kg, plus critical damping. This is the same idea as MuJoCo's own viewer.

**Performance.** In headless mode, the Rover in the line-track arena must run at least 20× real time on a 4-core laptop CPU from 2020 or later.

## Hardware models

Motors are modelled as DC motors with a gearbox, driven by a battery that sags under load. Sensors sample at real update rates, add noise and delay, and have real blind spots. Every number in this section is a placeholder that must be calibrated. Keep them all in `assets/physics/hardware.ron`, marked `placeholder: calibrate`.

**Battery** (inside the hub)

- Two-cell lithium-ion pack: 8.4 V full, 7.4 V nominal, 6.4 V empty. Open-circuit voltage falls linearly with state of charge.
- Internal resistance 0.15 Ω, capacity 2.0 Ah, hub idle draw 0.08 A. Missions start at 90% charge unless they say otherwise.
- Each step, terminal voltage = open-circuit voltage − internal resistance × total current. Total current is the idle draw plus the absolute value of every motor current from the previous step. Charge falls by current × dt.

**DC motor model** (all values at the output shaft, after the gearbox)

```latex
\tau = \frac{\tau_s}{V_{nom}}\,V \;-\; \frac{\tau_s}{\omega_0}\,\omega \qquad\qquad I = \frac{V - V_{nom}\,\omega/\omega_0}{R}
```

Here τ is torque, V the applied voltage, ω the shaft speed, τ\_s the stall torque and ω\_0 the no-load speed at V\_nom, and R the winding resistance. The torque equation is the MuJoCo `<general>` actuator from the compiler section, with `forcerange = ±τ_s × 8.4 / V_nom`. The current I is computed in `axl-hw` for the battery and for `motor_current()`.

**Motor command modes**

| Mode | What `axl-hw` does every 1 ms |
| --- | --- |
| Power p in \[−1, 1\] | V = p × battery voltage (like PWM duty) |
| Speed ω\_ref | Built-in PI loop on encoder speed: V = V\_nom/ω\_0 × ω\_ref + Kp e + Ki ∫e, clamped to battery voltage, with anti-windup. Defaults Kp = 0.6 V·s/rad, Ki = 6 V/rad. |
| Position θ\_ref, max speed | P loop on angle gives a speed target, limited to max speed, fed to the speed loop. Done when error < 1° and speed < 0.05 rad/s. |
| Brake | V = 0. Back-EMF brakes the motor. |
| Coast | V = the motor's own back-EMF at its current speed, so current and torque are about zero |

**Motor parameters** (placeholder: calibrate)

| Parameter | Medium motor | Large motor | Servo |
| --- | --- | --- | --- |
| Nominal voltage V\_nom | 7.4 V | 7.4 V | 7.4 V |
| No-load speed ω\_0 | 26.2 rad/s (250 rpm) | 18.3 rad/s (175 rpm) | 6.2 rad/s |
| Stall torque τ\_s | 0.15 N·m | 0.40 N·m | 0.25 N·m |
| Winding resistance R | 6.0 Ω | 4.0 Ω | 5.0 Ω |
| Armature (reflected inertia) | 1.5e-4 kg·m² | 3.0e-4 kg·m² | 1.0e-4 kg·m² |
| Gearbox friction loss | 0.004 N·m | 0.008 N·m | 0.006 N·m |
| Encoder | 360 counts/rev | 360 counts/rev | 1024 counts over 300° |
| Travel | continuous | continuous | ±90° (joint limits) |
| Mass | 0.035 kg | 0.080 kg | 0.040 kg |

The servo is the same motor model with its own built-in position loop. **Encoders** report whole counts only: angle = floor(θ × CPR / 2π). Reported speed is the change in counts over the last 10 ms, as on real smart motors.

**Sensors**

| Sensor | Range | Update rate | Delay | Noise and limits |
| --- | --- | --- | --- | --- |
| Ultrasonic distance | 0.03–2.5 m | 20 Hz | 30 ms | Cone of 13 rays: the centre ray plus rings of 6 at 7.5° and 15°. Returns the nearest hit whose surface faces within 40° of the ray, so angled walls are missed. σ = 3 mm + 1% of distance, rounded to 1 mm. Returns −1 when nothing is in range. |
| Time-of-flight distance | 0.04–2.0 m | 50 Hz | 20 ms | 5 rays (centre plus 4 at 2.5°), works up to 75° off-normal. σ = 2 mm + 0.5% of distance. Returns −1 when nothing is in range. |
| Colour | surface 4–30 mm away | 100 Hz | 10 ms | Averages 5 samples over a 3 mm-radius spot, so line edges read gradually. Intensity falls as 1 / (1 + ((d − 8 mm) / 15 mm)²). Outputs RGB 0–1023, reflectance 0–100, and the nearest of: none, black, white, red, green, blue, yellow. σ = 1.5% of full scale. |
| Touch | 0–10 N | 100 Hz | 5 ms | Pressed when the plunger's normal force exceeds 1.0 N, debounced over 5 ms. Force reading σ = 0.05 N. |
| Gyroscope (hub) | ±34.9 rad/s | 100 Hz | 10 ms | Per sample σ = 0.003 rad/s. Bias starts at N(0, 0.005 rad/s) per axis and drifts by 1e-5 rad/s per sample. 16-bit quantisation. |
| Accelerometer (hub) | ±78.5 m/s² | 100 Hz | 10 ms | Per sample σ = 0.02 m/s², bias N(0, 0.05 m/s²). 16-bit quantisation. |

The hub gives no heading or tilt. Combining gyro and accelerometer data is the learner's job, and it is a mission.

**Sample, hold and delay.** Each sensor samples at its update rate and keeps a queue of timestamped readings. A read at time t returns the newest reading taken at or before t minus the delay. The colour sensor gets the floor colour by mapping the ray's hit point to the mat texture's UV coordinates. On any other surface it uses that geom's colour.

**Friction and joints** (placeholder: calibrate). MuJoCo uses the larger friction value of two touching geoms, so the values below are chosen with that in mind.

| Surface or joint | Value |
| --- | --- |
| ABS plastic | sliding friction 0.35 |
| Rubber tyre | sliding 1.0, torsional 0.005, `condim="4"` |
| Steel caster ball | sliding 0.2, held by an internal ball joint with damping 1e-6 N·m·s/rad, so it rolls |
| Floor mat (vinyl) | sliding 0.5 |
| Arena walls (painted wood) | sliding 0.4 |
| Smooth pin joint | damping 1e-5 N·m·s/rad, friction loss 0 |
| Friction pin joint | damping 1e-5 N·m·s/rad, friction loss 0.002 N·m |
| Axle in a round hole | damping 1e-5 N·m·s/rad, friction loss 0.0005 N·m |
| Each gear mesh | adds friction loss 0.0005 N·m to the driven joint |

## Controller runtime and robot API

A learner's program is an ordinary C or Rust program compiled to a `wasm32-wasip1` module. Its `main` loops, calling `axl_step(ms)` to let time pass, just like firmware on a real robot. `sdk/c/axl.h` is the authoritative definition of the interface, and the Rust crate `axl-robot` must mirror it exactly.

**How a controller runs**

1. AXL instantiates the module in wasmtime on the sim thread and calls its `_start`.
2. The program's first AXL call must be `axl_init(1)`. Any other call first stops the program with: "Call axl\_init before anything else."
3. Each `axl_step(ms)` runs `ms` physics steps (1 to 1000), then returns `AXL_RUNNING` (0), or `AXL_MISSION_OVER` (1) when the mission has ended.
4. When `main` returns, every motor brakes and the simulation runs on until the mission ends.

**Sandbox and limits**

- Linear memory: 64 MB at most.
- **Step watchdog.** Epoch interruption ticks every 100 ms of wall-clock time. If 2 s pass without a call to `axl_step`, stop the program with: "Your program ran for 2 seconds without calling axl\_step. A loop may be missing a step."
- **WASI.** stdout and stderr go to the log panel, so `printf` and `println!` work. There is no filesystem, no environment and no network. Clocks return sim time and `random_get` returns seeded bytes.
- **Errors.** A wrong handle or the wrong device kind stops the program with a plain message, for example: "axl\_motor\_power: device 3 (eye) is a distance sensor, not a motor." Enable wasmtime's DWARF backtrace details, so every error shows the learner's source file and line.

**Functions** (all imported from wasm module `axl`; `dev` is a handle from `axl_device`)

| Group | Functions |
| --- | --- |
| Lifecycle | `int32_t axl_init(int32_t abi)`, `int32_t axl_step(int32_t ms)`, `double axl_time(void)` |
| Devices | `int32_t axl_device(const char* name, int32_t len)` returns a handle, or −1 if not found. `int32_t axl_device_kind(int32_t dev)` |
| Motor commands | `axl_motor_power(dev, double p)`, `axl_motor_speed(dev, double rad_s)`, `axl_motor_position(dev, double rad, double max_rad_s)`, `axl_motor_brake(dev)`, `axl_motor_coast(dev)`, `axl_motor_reset_angle(dev)` |
| Motor readings | `double axl_motor_angle(dev)`, `double axl_motor_rate(dev)`, `double axl_motor_current(dev)`, `int32_t axl_motor_done(dev)` |
| Distance | `double axl_distance(dev)` returns metres, or −1 when nothing is in range |
| Colour | `void axl_color_rgb(dev, int32_t* rgb3)`, `int32_t axl_color_reflect(dev)`, `int32_t axl_color_name(dev)` |
| Touch | `int32_t axl_touch_pressed(dev)`, `double axl_touch_force(dev)` |
| Hub | `void axl_imu_gyro(double* xyz3)`, `void axl_imu_accel(double* xyz3)` (robot frame), `double axl_battery_voltage(void)` |
| Teaching | `axl_plot(const char* channel, int32_t len, double value)` (up to 16 channels), `axl_mark_pose(double x, double y, double yaw)` (draws the robot's belief as a ghost trail), `axl_draw_line(x0, y0, z0, x1, y1, z1, uint32_t rgba)`, `axl_draw_clear(void)` |
| Missions | `axl_report(const char* key, int32_t len, double value)` for missions that ask the robot to measure something |

In the header, declare each import with `__attribute__((import_module("axl"), import_name("<name without axl_>")))`. Add `static inline` helpers that take C strings and compute the length, such as `axl_device_named(const char*)`. Define `AXL_ABI_VERSION 1`, the status codes, the device kinds and the colour names as `enum`s.

**C example** (`templates/c/m01/main.c`)

```c
#include "axl.h"

int main(void) {
    axl_init(AXL_ABI_VERSION);
    int left = axl_device_named("left");
    int right = axl_device_named("right");
    int eye = axl_device_named("eye");
    while (axl_step(10) == AXL_RUNNING) {
        double d = axl_distance(eye);
        double p = (d > 0.0 && d < 0.20) ? 0.0 : 0.5;
        axl_motor_power(left, p);
        axl_motor_power(right, p);
    }
    return 0;
}
```

**Rust example** (`templates/rust/m01/src/main.rs`)

```rust
use axl_robot::Robot;

fn main() {
    let mut robot = Robot::new();            // calls axl_init(1)
    let left = robot.motor("left");
    let right = robot.motor("right");
    let eye = robot.distance_sensor("eye");
    while robot.step(10) {                   // false once the mission is over
        let p = match eye.distance() {
            Some(d) if d < 0.20 => 0.0,
            _ => 0.5,
        };
        left.power(p);
        right.power(p);
    }
}
```

The Rust crate wraps handles in types (`Motor`, `DistanceSensor`, `ColorSensor`, `TouchSensor`, `Imu`). It returns `Option<f64>` where C returns −1, and panics with a clear message when a device name is not found.

**Building**

- **Rust:** `cargo build --target wasm32-wasip1 --release`, with `debug = "line-tables-only"` in the template's release profile.
- **C:** `clang --target=wasm32-wasip1 --sysroot=$WASI_SDK_PATH/share/wasi-sysroot -std=c17 -O2 -g -Wall -Wextra -I<sdk>/c main.c -o robot.wasm`, using the clang that comes with wasi-sdk. Pin the tested wasi-sdk version in `docs/toolchains.md`.
- **Toolchain check.** The Code screen checks for `rustup` with the `wasm32-wasip1` target installed, and for wasi-sdk (via `WASI_SDK_PATH`, or `/opt/wasi-sdk`, `~/wasi-sdk`, `C:\wasi-sdk`). If something is missing, show the exact install steps. Offer a button that runs `rustup target add wasm32-wasip1`.
- **Projects.** Each mission's code lives in `<data dir>/projects/<mission>/<c|rust>/`, copied from `templates/`. AXL watches the folder with the `notify` crate, so any external editor works. Compiler errors are parsed into file, line and column, and shown as clickable markers.
- **Built-in editor.** An egui text editor with C and Rust syntax highlighting, line numbers and error markers, plus an "Open in my editor" button. It is a convenience, not an IDE.
- **SDK check.** `cargo xtask sdk-check` builds every template in both languages and runs each headless against its mission. CI runs it.

## Rendering and realism

The target look is a product photograph of real plastic bricks on a studio floor: glossy moulded plastic, bevelled edges catching the light, soft shadows, and parts that sit firmly on the ground. Realism comes from four things, in this order: edge bevels, image-based lighting, contact shadows with ambient occlusion, and small per-part imperfections.

**Shader language.** GPU code must be WGSL, because that is what Bevy and wgpu run. This is the one exception to Rust-and-C-only. Keep each shader small and commented.

**Lighting**

- **Environment light.** Use Bevy's environment-map light from a neutral indoor studio HDRI from Poly Haven (CC0, 2K). If Bevy 0.20 can filter environment maps at runtime, use that. Otherwise, write `cargo xtask envmap` in Rust to convert the HDRI into prefiltered diffuse and specular KTX2 cubemaps, and record the choice in a decision record.
- **Key light.** One directional light at 50° elevation with soft cascaded shadows. Fill comes from the environment map only.
- **Grounding.** Bevy contact shadows plus screen-space ambient occlusion. Without them, small parts look like they float.
- **Tuning.** Use AgX tonemapping and Bevy's physical camera exposure. Keep the final light values in `assets/render/lighting.ron`.

**Materials** (Bevy `StandardMaterial`)

| Material | Settings |
| --- | --- |
| ABS plastic | perceptual roughness 0.28, metallic 0, reflectance 0.5, clearcoat 0.15, clearcoat roughness 0.2 |
| Transparent ABS | specular transmission 0.9, IOR 1.58, thickness 1.2 mm, roughness 0.1 |
| Rubber tyre | base #1A1A1A, roughness 0.85, reflectance 0.35, tread pattern in the mesh |
| Steel (caster ball) | metallic 1, roughness 0.15, base #BFBFBF |
| Floor mat | matte vinyl, roughness 0.75, printed mission graphics as its colour texture |

**Imperfections**

- Bevel every outer edge: 0.15 mm on bricks and plates, 0.3 mm on beams. These highlights are the strongest realism cue.
- Vary each part slightly, seeded by its `uid`: roughness ±0.03 and brightness ±1.5%.
- Apply a tiling, procedurally generated 256×256 micro-surface normal map at strength 0.15.

**Colours** (sRGB): white #F2F3F2, light grey #A0A5A9, dark grey #6C6E68, black #1B2A34, red #C91A09, blue #0055BF, yellow #F2CD37, green #237841, orange #FE8A18.

**Meshes.** `axl-parts` generates every part mesh at build time as plain vertex data, which `axl-render` uploads once per part type. Gears get involute-approximated teeth at module 1 mm, and tyres get a tread pattern. Parts of the same type and colour share one mesh and one material, so Bevy batches them automatically.

**Mission mats.** Draw each mat at runtime from its world definition, at 2 pixels per mm, using the `tiny-skia` crate (anti-aliased paths). Lines, mazes and zones are vector shapes in the world file. Compress the result with Bevy's texture compressor.

**The grid floor**

- An effectively infinite floor (a 200 m quad) using a custom WGSL material. Grid lines are drawn analytically in the shader and anti-aliased with screen-space derivatives (`fwidth`).
- **Stud mode** (default): minor lines every 8 mm (1 stud), major lines every 80 mm (10 studs), and bold lines every metre. **Metric mode**: 10 mm, 100 mm and 1 m.
- Minor lines fade out when they are closer than 6 pixels apart on screen. Every line fades with distance.
- At the origin, the X axis is drawn red and the Y axis green. A small XYZ gizmo sits in the corner of the view.
- Lines darken the floor colour before lighting, so they receive shadows and occlusion like printed ink, rather than glowing on top.
- Grid opacity can be set from 0 to 100%. Defaults: 60% in the builder, 25% while simulating.
- **Builder extras.** Stud-position dots on the surface under the cursor, and a measure tool: click two points to see the distance in mm and studs.

**Camera.** Perspective with a 35° field of view (less distortion, like product photography), near plane 5 mm, far plane 100 m. Modes: orbit (Bevy's pan-orbit camera), follow robot, and top-down orthographic for mat missions.

**Quality presets** (chosen by a 3-second benchmark on first launch; the user can change it)

| Setting | Low | Medium | High | Ultra |
| --- | --- | --- | --- | --- |
| Render scale | 0.75 | 1.0 | 1.0 | 1.0 |
| Anti-aliasing | FXAA | SMAA | TAA | TAA |
| Shadows | 1 cascade, 1024 px | 2 cascades, 2048 px | 4 cascades, 2048 px, soft | 4 cascades, 4096 px, soft |
| Contact shadows | Off | Off | On | On |
| Ambient occlusion | Off | Low | Medium | High |
| Bloom | Off | On | On | On |
| Micro-surface detail | Off | On | On | On |
| Target | 30 fps, integrated graphics | 60 fps 1080p, entry GPU | 60 fps 1080p, mid-range GPU | 60 fps 1440p, high-end GPU |

**Look-dev scene.** Ship `assets/scenes/lookdev.ron`: a 2×4 brick, a 7-hole beam, a 24-tooth gear, a 56 mm wheel and the Rover on the grid floor. `axl --lookdev --screenshot out.png` renders it at High. The owner approves the look from these screenshots, side by side with photos of real bricks.

## Teaching overlays and instruments

Overlays make the invisible physics visible. Each one can be switched on by itself, all of them draw from the latest `SimSnapshot`, and none of them changes the simulation.

| Overlay | Shows | Key |
| --- | --- | --- |
| Frames | 30 mm RGB axes on every body, plus the world gizmo | `1` |
| Balance | The centre of mass as a sphere with a line dropped to the floor, and the support polygon (convex hull of the floor contacts). The dropped point is green inside the polygon and red outside. Uses MuJoCo `subtree_com` of the root body. | `2` |
| Contacts | Contact points as dots and normal forces as arrows (10 mm per newton, logarithmic above 5 N). On High and above, friction cones as wireframes. | `3` |
| Sensors | Ultrasonic cones, time-of-flight rays, the colour sensor's spot, ray hit points, and each sensor's live reading as a label | `4` |
| Trails | The robot's true path as a solid line, its believed path from `axl_mark_pose` as a dashed line, and a ghost outline of the robot at its believed pose. Last 60 s. | `5` |
| Drawings | Lines drawn by the controller with `axl_draw_line` | `6` |
| X-ray | Parts at 30% opacity, so joints and gears show through | `7` |

**Inspector.** Clicking a body shows its mass, centre of mass, inertia and velocities. Hovering a joint shows its angle, speed and torque.

**Plotter** (an `egui_plot` panel)

- Channels: everything the controller sends with `axl_plot`, plus built-in channels to pick from. Built-ins are each motor's angle, speed and current; battery voltage; each sensor's reading; and the robot's true x, y and yaw.
- Time window of 5, 10, 30 or 60 s, a pause button, and a cursor readout.
- Export the visible channels to CSV through a save dialog (`rfd` crate).

**Time controls**

| Action | Control |
| --- | --- |
| Play or pause | `Space` |
| Step 1 ms or 10 ms while paused | `.` and `Shift+.` |
| Speed 0.1×, 0.25×, 0.5×, 1×, 2×, 4× | Buttons, or `[` and `]` |
| Rewind | Scrub bar (deterministic replay) |
| Reset to the start | `Ctrl+R` |

## UI screens and controls

There are seven screens, all built with egui on top of the 3D view. The UI uses a dark neutral theme with one accent colour (orange #FE8A18). Fonts are Atkinson Hyperlegible for UI text and JetBrains Mono for code, bundled under the SIL Open Font License, which is the one licence exception for assets.

| Screen | Layout and contents |
| --- | --- |
| Home | Continue (last mission), Missions, Sandbox, Settings. Shows overall progress. |
| Missions | Missions grouped by chapter, with status, stars and concept tags. The detail view shows the brief, the goal, the rules, the starter robot, and the exact success criteria before starting. |
| Builder | Left: part palette with search, categories and colour swatches. Centre: 3D view. Right: inspector for the selected part, including device name and port. Bottom: validation panel. Top bar: Undo, Redo, Save, Load, Simulate. |
| Code | Left: project files. Centre: editor. Right: searchable API reference generated from the doc comments in `axl.h`. Bottom: build log. Toolbar: C or Rust, Build, Build and Run, Open in my editor, and a toolchain status chip. |
| Simulate | Centre: 3D view. Top: time controls. Left: overlay toggles. Right: plotter, log and live device readings. Mission heads-up display: timer and an objectives checklist. |
| Results | Pass or fail for each criterion, metrics (time, energy used in Wh, collisions), stars, a replay button and one hint for what to try next |
| Settings | Quality preset, units (studs or mm), grid opacity, UI scale (100–200%), invert orbit, toolchain paths, data folder |

**Camera controls** (builder and simulate)

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Orbit | Right-drag, or Alt + left-drag |  |
| Pan | Middle-drag, or Shift + right-drag |  |
| Zoom | Wheel |  |
| Focus on selection |  | `F` |
| Cycle orbit, follow and top-down |  | `V` |
| Reset view |  | `Home` |

**Accessibility.** The UI scales from 100% to 200%. Overlays never rely on colour alone: the balance overlay also shows a tick or a cross. Every action in the tables above has a keyboard shortcut or a visible button.

## Worlds and physical objects

Version 1 ships six worlds and seven standard objects. A world is a RON file in `assets/worlds/` that describes the floor mat, walls, static shapes, loose objects, the spawn pose, and named zones used for grading. The compiler adds the world to the same MJCF as the robot.

```rust
pub struct WorldDef {
    pub id: String,
    pub name: String,
    pub mat: Option<MatDef>,              // printed graphics on the floor; the floor plane itself is infinite
    pub walls: Vec<WallDef>,              // from [x, y], to [x, y], height, thickness, material
    pub statics: Vec<StaticShape>,        // ramps, steps, posts: primitive, pose, material
    pub objects: Vec<ObjectPlacement>,    // object id from the library, pose
    pub spawn: Pose2,                     // x, y, yaw
    pub zones: Vec<Zone>,                 // named rectangles or circles for grading; not physical
}
pub struct MatDef { pub size_m: [f64; 2], pub background: Rgb, pub shapes: Vec<MatShape> } // Line { points, width_mm, color } | Rect | Circle
```

**Version 1 worlds**

| Id | Contents |
| --- | --- |
| `open_grid` | Bare grid floor, no walls. The sandbox. |
| `test_track` | 2.0 × 1.2 m mat with distance marks every 100 mm and a start box |
| `line_track` | 2.0 × 1.2 m white mat with a 19 mm black line. Variant A: oval. Variant B: an S-bend and a 90° corner. |
| `walled_arena` | 1.5 × 1.5 m floor with 100 mm painted-wood walls and four box obstacles |
| `maze` | 6 × 6 cells of 250 mm, with 100 mm walls. Three fixed layouts, chosen by the run seed. |
| `ramp_lab` | Four ramps at 10°, 15°, 20° and 25°, two with rubber surfaces and two with plastic |

**Standard objects** (`assets/objects/`). Masses are typical real values; keep the source note with each one.

| Id | Shape | Mass | Friction | Notes |
| --- | --- | --- | --- | --- |
| `cube_wood_40` | 40 mm cube | 0.032 kg | 0.4 | Pine, about 500 kg/m³ |
| `cube_steel_40` | 40 mm cube | 0.25 kg | 0.3 | A heavy block for pushing tests |
| `box_card_100` | 100 mm cube | 0.05 kg | 0.5 | Empty cardboard box |
| `cylinder_wood_40x60` | 40 mm diameter, 60 mm tall | 0.038 kg | 0.4 | Pine |
| `ball_table_tennis` | 40 mm diameter | 0.0027 kg | 0.4 | Hollow: `shellinertia="true"` |
| `ball_tennis` | 67 mm diameter | 0.058 kg | 0.6 | Hollow: `shellinertia="true"` |
| `can_330` | 66 mm diameter, 115 mm tall | 0.36 kg | 0.3 | A full 330 ml drink can, treated as solid |

Objects are free bodies in geom group 2, so sensors can see them and robots can push them. Phase 2 adds grasping.

## Missions and curriculum

Version 1 has 12 missions that take a learner from driving a motor to PID control, sensor fusion and mechanical design. Every mission is graded automatically in headless mode, over several seeds where noise matters, so only robust code passes.

**Mission definition** (in `axl-missions`, files in `assets/missions/`)

```rust
pub struct MissionDef {
    pub id: String, pub title: String, pub chapter: u8, pub concepts: Vec<ConceptId>,
    pub world: WorldRef,                  // world id + variant
    pub robot: RobotSource,               // Starter(id) | Build { max_parts: u16, allowed: Vec<PartId> }
    pub time_limit_s: f64,
    pub grading_seeds: Vec<u64>,          // every seed must pass
    pub battery_soc: Range<f64>,          // start charge, drawn per seed
    pub disabled_api: Vec<String>,        // e.g. ["motor_speed", "motor_position"]
    pub events: Vec<MissionEvent>,        // e.g. Push { t, impulse_ns: [f64; 3] }
    pub brief_md: String,
    pub predict: Option<PredictPrompt>,   // asked before the first run
    pub criteria: Vec<Criterion>,         // all must hold
    pub stars: [StarRule; 2],             // the 2nd and 3rd stars; passing gives the 1st
    pub hints: [String; 3],
}
pub enum Criterion {
    ReachZone { zone: String, within_s: f64 },
    FinalPose { x: f64, y: f64, yaw: f64, pos_tol_m: f64, yaw_tol_rad: f64 },
    NoContact { tag: String },
    FollowLine { max_dev_m: f64, laps: u8 },
    MeanSpeed { from_s: f64, to_s: f64, target_ms: f64, tol_ms: f64 },
    Report { key: String, expected: Expected, tol: f64 },
    NoTip { max_tilt_rad: f64 },
}
```

A disabled API function stops the program with: "This mission switches off motor\_speed so you can build your own controller."

**The 12 missions**

| Id | Title | Concepts | World and robot | Pass when | 2nd and 3rd stars |
| --- | --- | --- | --- | --- | --- |
| m01 | First Drive | Actuators, power | `test_track`, Rover | The front stops between 0.95 and 1.05 m within 10 s | Stop within ±20 mm; no hints |
| m02 | Counting Turns | Wheel kinematics, encoders | `test_track`, Rover. Battery 60–100% per seed, so timing alone fails. | Final x = 0.75 m ± 10 mm on all 3 seeds | ±3 mm; under 4 s |
| m03 | Turn on the Spot | Differential drive, wheel base | `open_grid`, Rover | Turn 90° then 180°. Final yaw ±3°, drift < 20 mm. | ±1°; drift < 5 mm |
| m04 | Square Dance | Odometry, drift | `test_track`, Rover | Drive a 0.5 m square and end within 30 mm of the start | Within 10 mm; believed pose (`axl_mark_pose`) within 10 mm of the true pose |
| m05 | Mind the Wall | Range sensors, blind spots | `walled_arena`, Rover with either distance sensor | Stop 100 ± 10 mm from the wall without touching it. Seed 3 angles the wall at 50°. | ±3 mm; under 5 s |
| m06 | Filter the Noise | Noise, averaging, median | `walled_arena`, Rover standing still | Report the distance to the box within ±5 mm on all 3 seeds | ±2 mm; within 1 s |
| m07 | Hold the Speed | Feedback control (P, PI) | `ramp_lab` 15° ramp, Rover; `motor_speed` and `motor_position` disabled | Mean speed 0.25 ± 0.01 m/s from 2 s to 5 s, uphill | ±0.005 m/s; uses `axl_plot` |
| m08 | Line Follower I | On-off and P control | `line_track` A, Rover | One lap within 60 s, never more than 40 mm off the line | Lap under 30 s; deviation < 20 mm |
| m09 | Line Follower II | PID tuning | `line_track` B, Rover | One lap within 35 s, deviation < 25 mm | Under 20 s; deviation < 12 mm |
| m10 | Heading Lock | Gyro, bias, sensor fusion | `test_track`, Rover; side pushes at 0.5 s and 1.0 s | Drive 1.5 m. Lateral error < 30 mm, yaw error < 2°. | < 10 mm; < 0.5° |
| m11 | Escape the Maze | State machines, wall following | `maze`, Rover; 3 seeds = 3 layouts | Reach the goal cell within 120 s on every layout | Under 60 s; no wall contact |
| m12 | Torque and Tipping | Gear ratios, centre of mass | `ramp_lab` 25° ramp, builder unlocked, max 120 parts | Reach the top zone within 20 s without tilting past 45° | Under 10 s; under 60 parts |

**Learning design** (carried over from WYK)

- **Predict first.** Each mission asks one prediction before the first run. For example, m01: "At power 0.5 for 2 seconds, how far will the Rover travel?" The real answer is shown after the run.
- **Hints.** Three graduated hints, unlocked after the first failed run.
- **Spaced warm-ups.** Before each mission, ask one or two quick questions on earlier concepts that are due, scheduled with FSRS-4.5 (same parameters as WYK). The numbers in each question are generated, for example: "A 56 mm wheel turns 3 times. How far does the robot go?"
- **Feedback tone.** Failure messages say which criterion failed, by how much, and on which seed. Bugs are clues, not failures.

## Accuracy validation suite

AXL's accuracy claim rests on 18 automated tests. Each compares the full physics and hardware stack against a textbook answer, within a stated tolerance. `cargo xtask validate` runs them headless and writes `target/validation/report.md` and `report.json`. CI fails if any test fails, and the latest report ships inside the app under Settings → About → Accuracy report.

Work out each expected value in code from the model's actual parameters, such as its real inertia, never a hard-coded number. That way a test checks the physics rather than the arithmetic.

| Id | Test | Expected | Tolerance |
| --- | --- | --- | --- |
| V01 | Free fall from 1 m | Impact at t = √(2h/g) = 0.4515 s | ±1 ms; position error ≤ 2 mm at 0.3 s |
| V02 | Pendulum, 2° swing | Period T = 2π √(I / (m g d)) from the model's inertia | ±0.2% |
| V03 | Frictionless pendulum, 30°, 10 s | Total energy constant | Drift < 1% |
| V04 | Box on a slope with μ = 0.5 | Holds at 25.5°, slides at 27.5° (threshold atan 0.5 = 26.57°) | Creep ≤ 2 mm in 2 s at 25.5°; slides > 50 mm in 2 s at 27.5° |
| V05 | Box sliding from 1 m/s, μ = 0.4 | Stops after v² / (2μg) = 0.1275 m | ±3% |
| V06 | Solid cylinder rolling down 10° | a = (2/3) g sin θ = 1.135 m/s², so 0.5676 m in 1 s. Repeat for a sphere with a = (5/7) g sin θ. | ±1% |
| V07 | Medium motor, no load, at V\_nom | Steady speed ω\_0 (1 − τ\_f / τ\_s) | ±0.5% |
| V08 | Medium motor, shaft blocked | Torque τ\_s × V / V\_nom; current V / R | ±0.5% |
| V09 | Motor step response | Reaches 63.2% of steady speed at t = J ω\_0 / τ\_s | ±5% |
| V10 | 8-tooth gear driving 40-tooth, 10 turns | Output angle = −(8/40) × input angle | ±0.1% |
| V11 | Two large motors stalled | Battery voltage = open-circuit voltage − R\_int × total current | ±1% |
| V12 | Rover, equal wheel speeds, 1 m | Drives straight | Lateral drift < 1 mm |
| V13 | Rover, unequal wheel speeds | Turn radius R = (b/2)(ω\_R + ω\_L) / (ω\_R − ω\_L), where b is the wheel spacing | ±3% |
| V14 | Rover at rest | Sinks into the floor less than 0.2 mm | Hard limit |
| V15 | Same scene and seed, run twice | SHA-256 of all joint positions every 100 ms is identical | Exact, same machine |
| V16 | Distance sensors, noise off | Ultrasonic reads 0.500 m from a wall at 0.5 m and returns −1 when the wall is angled 50°. Time-of-flight still reads the angled wall. | ±1 mm |
| V17 | Colour sensor across a black-white edge | Reflectance crosses 50% at the edge | ±1 mm |
| V18 | Gyro, noise off, 1 rad/s for 5 s | Integrated angle = 5 rad; encoder readings are whole multiples of 2π/360 | ±0.1% |

**Calibration list.** `docs/calibration.md` lists every placeholder value from Hardware models and how to measure it on real hardware. For example, motor no-load speed with a tachometer, stall torque with a lever arm and a scale, and friction from the angle at which a part starts to slide. Calibration is owner work, not part of version 1 acceptance.

## Persistence and file formats

All user data is plain RON in one folder per user, written atomically and versioned. Shipped assets (parts, worlds, objects, missions) are read-only and live in the install folder.

**Data folder:** `directories::ProjectDirs::from("org", "axl", "AXL").data_dir()`

```text
<data>/
  settings.ron                 quality, units, grid opacity, UI scale, toolchain paths
  progress.ron                 per mission: best stars, attempts, completion date; FSRS state per concept
  robots/<name>.axlbot         saved assemblies
  projects/<mission>/<c|rust>/ learner code, copied from templates
  exports/                     MJCF exports and CSV plots
  logs/axl.log                 rolling log, last 5 files
```

**Rules**

- Every file has a top-level `version: u32`. Loaders migrate older versions. A newer version than the app knows is refused with: "This file was made by a newer version of AXL."
- Writes are atomic: write a temporary file in the same folder, then rename it. Keep the previous copy as `<file>.bak`.
- Autosave the robot 1 s after the last edit. Save progress as soon as a result appears.
- A robot is shared as one `.axlbot` file. A project is shared as its folder.

**Example `.axlbot`**

```ron
(
    version: 1,
    name: "Rover",
    parts: [
        (uid: 1, part: "hub", pos: (0, 0, 0), orient: 0, color: "white",
         device: Some((name: "hub", port: None))),
        (uid: 2, part: "motor_medium", pos: (-240, 320, 0), orient: 4, color: "light_grey",
         device: Some((name: "left", port: Some(A)))),
        (uid: 3, part: "plate_4x8", pos: (0, 0, -32), orient: 0, color: "dark_grey", device: None),
    ],
)
```

`pos` is in 0.1 mm steps. `port` is `None` only for the hub.

## Build, CI and release

One Cargo workspace builds everything, and `cargo xtask` handles the tasks Cargo alone can't. GitHub Actions tests every push on Windows, macOS and Linux, and builds release packages from version tags.

**Toolchain.** `rust-toolchain.toml` pins the current stable Rust at project start (1.95 or newer), with components `rustfmt` and `clippy` and target `wasm32-wasip1`.

**Commands**

| Command | Does |
| --- | --- |
| `cargo xtask fetch-mujoco` | Download and verify MuJoCo 3.12.0 for this OS |
| `cargo run -p axl-app --release` | Run AXL |
| `cargo test --workspace` | Unit and integration tests |
| `cargo xtask validate` | The 18-test accuracy suite and its report |
| `cargo xtask sdk-check` | Build every C and Rust template and reference solution, and run each against its mission headless |
| `cargo xtask package` | Build the release package for this OS |
| `cargo xtask envmap` | Only if needed: prefilter the HDRI into KTX2 cubemaps |

**CI** (`.github/workflows/ci.yml`, on every push and pull request)

- Matrix: `ubuntu-24.04`, `windows-2022`, `macos-14`.
- Linux system packages for Bevy: `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.
- Steps in order: install Rust; restore the Cargo cache; `fetch-mujoco`; download the pinned wasi-sdk; `cargo fmt --check`; `cargo clippy --workspace -- -D warnings`; `cargo test --workspace`; `cargo xtask validate`; `cargo xtask sdk-check`.
- CI never opens a window or needs a GPU. Headless mode skips the renderer entirely.

**Release** (`.github/workflows/release.yml`, on tags `v*`)

| OS | Package |
| --- | --- |
| Windows | `.zip`: `axl.exe`, `mujoco.dll`, `assets/`, `sdk/` |
| macOS | `AXL.app` in a `.dmg`, unsigned in version 1. The README explains how to open an unsigned app. |
| Linux | `.tar.gz` with an rpath-relative `libmujoco` |

Each release attaches the validation report. Reference solutions in `missions/solutions/` are used by CI but never shipped in packages.

**Branches.** Build each milestone on a branch `milestone/mNN`. Merge it into `main` only when its acceptance checks pass, and mark it accepted in `docs/STATUS.md`. Also create the tag `mNN-done`. If pushing the tag is refused, leave it local, list the push command as an owner check, and carry on: the merge commit and the status row are what count.

## Engineering standards

The codebase must stay warning-free, panic-free on user input, and limited to an approved list of crates. These rules apply to every crate.

**Code**

- `cargo fmt` defaults. `cargo clippy -- -D warnings` must pass.
- `#![forbid(unsafe_code)]` in every crate except `axl-mujoco`. Library crates also set `#![warn(missing_docs)]`.
- **Errors.** Library crates use `thiserror`; only `axl-app` and `xtask` use `anyhow`. No `unwrap()` or `expect()` outside tests, unless a comment shows why it cannot fail.
- **No panics on user input.** A bad robot, file or controller turns into a message in the UI, never a crash.
- **Logging.** Use `tracing`. Never log inside the per-step loop above `trace` level.
- **Hot loop.** No heap allocation in the 1 ms step loop of `axl-sim` and `axl-hw`. Preallocate buffers when the simulation starts.

**Approved crates.** Anything else needs a decision record first.

| Purpose | Crates |
| --- | --- |
| Engine and UI | `bevy`, `bevy_egui`, `egui_plot`, `egui_extras` (syntax highlighting) |
| Physics and sandbox | `mujoco-rs`, `wasmtime`, `wasmtime-wasi` |
| Data | `serde`, `ron`, `sha2`, `image` |
| Errors and logs | `thiserror`, `anyhow`, `tracing`, `tracing-subscriber` |
| Threads | `crossbeam-channel`, `triple_buffer` |
| Randomness | `rand`, `rand_chacha` |
| Desktop | `notify`, `directories`, `rfd`, `clap` |
| Graphics helpers | `tiny-skia`, `glam` |
| Tests only | `approx`, `proptest`, `insta`, `wat` |

**Tests**

- Unit tests next to the code in every crate.
- **Property tests** (`proptest`): the builder never accepts an overlapping placement; connections are symmetric; the compiler produces identical MJCF when part order is shuffled.
- **Snapshot tests** (`insta`): the MJCF of each starter robot.
- **Host ABI tests:** tiny WebAssembly modules written as WAT text, one per function, including every error message.

**Documentation.** Each crate has a short `README.md` saying what it owns. Keep `docs/architecture.md` in step with the code.

**Commits.** Small commits with conventional messages, for example `feat(compile): merge coaxial hinge links`.

## Milestones

Version 1 is 11 milestones, M0 to M10, built strictly in order. Each one ends with checks a machine or the owner can verify. Do not start a milestone until the previous one is merged into `main` and marked accepted in `docs/STATUS.md`.

| Milestone | Scope | Accepted when |
| --- | --- | --- |
| M0 Skeleton | Workspace with every crate, `rust-toolchain.toml`, `xtask` with `fetch-mujoco`, CI on 3 OSes, README build steps | CI is green on all 3 OSes. `axl` opens an empty window. `axl --headless --version` prints the version. |
| M1 Physics core | `axl-mujoco` safe API: load, step, state, rays with surface normals, contacts, warnings. `axl-validate` harness. | V01–V06 and V15 pass (using simple test scenes). Every public function has a unit test. No `unsafe` outside `axl-mujoco`. |
| M2 Rendering and grid | Lighting, materials, contact shadows, ambient occlusion, tonemapping, grid floor, camera, quality presets and benchmark, body sync from snapshots | `--lookdev --screenshot` works with placeholder shapes. Grid fading, axes and unit toggle behave as specified. 200 boxes hold 60 fps on High on a mid-range GPU (manual check, recorded in `docs/perf.md`). |
| M3 Parts | Part types, RON loader, all 75 part files, procedural meshes with bevels, gear teeth and tyre tread, volume and mass, a debug part browser | All 75 parts load and render. Computed volumes match hand calculations within 1% for 5 sample parts. The owner approves the look-dev screenshot. |
| M4 Builder | Assembly, snapping, connection derivation, validation, undo and redo, `.axlbot` save and load, the two starter robots | Property tests pass. The Rover can be built from scratch in the UI. Save then load gives an identical file. |
| M5 Compiler | Every algorithm step, MJCF export, `BodyMap` rendering, sandbox drive with fixed voltages | Snapshot tests pass. Shuffled part order gives identical MJCF. A four-bar linkage works. The gear-parent error shows. Exported MJCF loads in MuJoCo 3.12.0. V10 and V12–V14 pass. |
| M6 Hardware | Battery, motor modes, encoders, servo, all sensors with noise, delay and blind spots | V07–V09, V11 and V16–V18 pass. Over 10,000 samples, each sensor's measured noise is within 10% of its spec. |
| M7 Controllers | wasmtime host and the full ABI, watchdog, WASI shims, source-line errors, disabled API. `axl.h`, the `axl-robot` crate, templates, toolchain check, build pipeline, Code screen. | A WAT test covers every ABI function. The m01 templates in C and Rust drive the Rover. An endless loop shows the watchdog message within 2.5 s. A wrong-device call shows the file and line. |
| M8 Instruments | Time controls, replay rewind, overlays 1–7, inspector, plotter with CSV export, mouse drag | Rewinding to t and replaying gives the same joint-position hash. Every overlay toggles. Exported CSV opens correctly. |
| M9 Missions | Mission framework, criteria, events, grading seeds, headless grading, 6 worlds, 7 objects, 12 missions with predictions, hints and warm-ups, reference solutions in C and Rust, Results screen, progress saving | `sdk-check` passes all 24 reference solutions. Each criterion is unit-tested on synthetic paths. A deliberately weak solution fails every mission. |
| M10 Release | Home, Missions and Settings screens; accessibility; packages for 3 OSes; release workflow; `docs/guide.md`; accuracy report in About | Tag `v1.0.0` produces 3 packages with the validation report attached. A first-time tester finishes m01 in under 30 minutes. |

**Later phases** (not part of version 1)

- **Phase 2, manipulation.** Bevel and worm gears, gripper parts, self-collision between bodies that aren't directly jointed, and missions for arm kinematics (two-link analytic, then Jacobian), pick-and-place and stacking.
- **Phase 3, autonomy.** A camera sensor, a 2D lidar, and missions for mapping, particle-filter localisation, A\* and RRT planning, and a self-balancing robot (LQR). Also breakable connections and competitions (mini-sumo, warehouse sorting).
- **Phase 4, reach.** A browser build (WebGPU, MuJoCo's WebAssembly build, controllers running in the browser's WebAssembly engine), a ROS 2 bridge, and signed installers.

## Risks and mitigations

The biggest risks are library APIs newer than the implementing model's training, and contact stability with very light parts. Each risk below has a fixed response, so work doesn't stall.

| Risk | Mitigation |
| --- | --- |
| Bevy 0.20, wasmtime 50 or mujoco-rs 6.1 differ from what the implementing model remembers | Read the version's own examples and docs (`cargo doc --open`, the crate's `examples/`) before writing code against it. Never guess an API. Keep Bevy code inside `axl-app` and `axl-render`. |
| `mujoco-rs` doesn't wrap a needed function (ray with surface normal, contact force, state save) | Use its raw bindings inside `axl-mujoco`. If those are missing too, generate bindings for just that function and write a decision record. |
| Very light bodies make contacts unstable | The pin-cluster merge rule, plus a minimum body mass of 1 g. When the compiler adds mass to reach it, log it and show it in the inspector. |
| Soft contacts creep (V04) or sink (V14) | Tune `solref`, `solimp` and `impratio` first. If that fails, try MuJoCo's `noslip_iterations` and record the choice. |
| Learners struggle to install wasi-sdk | A setup screen with exact steps per OS, and Rust as the easier first path. Consider bundling in Phase 4. |
| Weak or unusual GPUs | First-launch benchmark, the Low preset, and wgpu backend fallback (DX12 or Vulkan, Metal, GL) |
| Results differ slightly across operating systems | Grading uses tolerances; bit-exact replay is promised only on the same machine |
| Placeholder hardware values differ from real motors and sensors | Every value lives in RON with its source, and `docs/calibration.md` says how to measure it. The accuracy report separates "physics verified" from "hardware calibrated". |
| The scope is too large for one pass | Strict milestone order. Each merged milestone runs on its own. When blocked, stop and report rather than skipping ahead. |
| Trademark confusion | Generic names, our own colour names, and no logo on studs |

## Open decisions for the owner

None of these blocks version 1. The implementing model uses the default until the owner decides otherwise.

| Decision | Default until decided |
| --- | --- |
| Final product name | AXL, repository `axl-sim` |
| Link with WYK | Standalone. A later option: finishing WYK unlocks a bonus mission. |
| School Chromebooks | Not supported until the Phase 4 browser build |
| macOS signing and notarisation (needs an Apple Developer account) | Unsigned packages, with opening instructions in the README |
| Which real motors and sensors to calibrate against | None. Keep the placeholder values, clearly marked. |
| Teacher review of mission content | The owner reviews each mission brief before M9 is accepted |

## Glossary

| Term | Meaning |
| --- | --- |
| ABI | The exact set of functions and data layouts shared by the simulator and a learner's compiled program |
| Actuator | The MuJoCo element that applies a motor's torque to a joint |
| Armature | Extra rotational inertia added to a joint. Here, the motor's rotor inertia as seen through its gearbox. |
| Assembly | The robot as the builder stores it: placed parts only. Connections are derived from it. |
| Back-EMF | The voltage a spinning motor generates. It opposes the supply, so torque falls as speed rises. |
| Cluster | A set of rigidly connected parts. It becomes one MuJoCo body. |
| Connector | A point on a part where another part can attach: stud, anti-stud, hole, pin or axle |
| Controller | The learner's program, compiled to WebAssembly, that drives the robot |
| Determinism | The same inputs always give exactly the same run |
| Epoch interruption | wasmtime's way of stopping a WebAssembly program that runs too long |
| FSRS | Free Spaced Repetition Scheduler, the memory model that schedules warm-up questions |
| Lockstep | The controller and physics take turns: the controller runs, then time advances by exactly the amount it asked for |
| MJCF | MuJoCo's XML model format |
| Module (gear) | Gear tooth size. At module 1 mm, pitch diameter in mm equals the tooth count. |
| RON | Rusty Object Notation, a readable data format for Rust structures |
| Sample and hold | A sensor reading that only changes at the sensor's update rate |
| Stall torque | A motor's torque when its shaft is held still at nominal voltage |
| Support polygon | The outline around a robot's floor contacts. The robot tips if its centre of mass passes outside it. |
| WASI | The WebAssembly System Interface: standard input/output, clocks and randomness for WebAssembly programs |
| WAT | The text form of WebAssembly, used for small host tests |
