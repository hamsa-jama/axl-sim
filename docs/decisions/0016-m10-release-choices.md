# 0016: M10 release choices

Status: accepted. Date: 2026-11-10.

## Context

M10 turns the app from per-flag views into the seven screens of the spec, puts the mission's world
on the 3D stage, and builds and releases the packages. These are the choices the spec left open,
the owner's confirmations, and the problems found in the spec.

## Owner confirmation

- **FSRS-4.5 weights and retention.** The warm-up scheduler uses the published FSRS-4.5 default
  weights (17 values, pinned by a unit test in `crates/axl-missions/src/learn/fsrs.rs`) and a
  target retention of 0.9. The owner confirmed both. They are not tuned to AXL's data, because
  there is none yet.

## Decisions

- **Version 1.0.0.** The workspace version is `1.0.0`. The release workflow refuses a tag that does
  not equal `v` plus the Cargo version.
- **Screen state machine.** One `Nav` resource says which of Home, Missions, Builder, Code,
  Simulate, Results and Settings is showing. Screens ask for work with `Nav::pending`; `apply_nav`
  does it. Existing flags keep working: `--builder`, `--sandbox`, `--code` choose the first screen
  and (for `--code`) a pending open. New flags `--screen`, `--detail` and `--grade` exist so the
  screens can be photographed and tested without a mouse. `--lookdev`, `--parts` and
  `--stress-boxes` keep their own stage and do not use the screens.
- **`--code` reads `settings.ron`** like a plain start (quality, units, UI scale, toolchain
  paths). It was treated as a scripted run that ignored the file. Scripted runs that still ignore
  it are `--lookdev`, `--parts`, `--stress-boxes`, `--perf-seconds`, `--sandbox` and
  `--screenshot`, because their output must not depend on the machine's settings.
- **Sandbox = the Rover on `open_grid` through the mission path.** `library::sandbox()` is a
  mission with no criteria, an hour of simulated time and no events, so the sandbox and the
  missions cannot drift apart. It has voltage sliders per motor and no code (the Code screen
  belongs to missions). The builder's **Simulate** button runs the design in the same sandbox.
- **Mission world on the stage.** Build and Run, and **Watch seed N in 3D**, run through
  `MissionSetup::spawn_live`: the same world, robot, scenario, battery and seed as a graded run.
  The live view uses the first grading seed and shows it in the Code toolbar and on the Simulate
  panel. A live test (`crates/axl-sim/tests/live.rs`) compares the end pose of a live run with a
  graded run of the same seed.
- **What is drawn comes from the physics numbers.** `axl-world::build` returns shape and object
  visuals from the same values it writes to the MJCF. The mat is a texture rasterised at 1 mm
  per pixel (linear colour). Zones are drawn only if a criterion or star rule names them.
- **Objectives come from the last graded result**, not from live evaluation, so the live view can
  never disagree with grading.
- **A mission that needs your robot (m12) before one is saved** shows the Rover in its world and
  says so; grading still needs a saved design.
- **Chapter names:** "Chapter 1: Make it move", "Chapter 2: Sense the world", "Chapter 3: Close the
  loop", "Chapter 4: Think and build".
- **Settings: toolchain paths.** `wasi_sdk_path` and `cargo_path` in `settings.ron`, empty meaning
  "search as before". They are applied through `axl_host::toolchain::set_overrides`, so the Code
  screen's toolchain chip and the build use them. The data folder is shown with a Copy button
  (opening a file manager is not portable without another crate).
- **About** renders the accuracy report found beside the program (`accuracy-report.md`, from the
  package) or in `target/validation/report.md` of a checkout.
- **Prediction audit (work item 4).** Every mission has a test (`crates/axl-sim/tests/predictions.rs`)
  that compares the measured number behind its prediction question with an independent oracle.
  Two mismatches were fixed: m05's measurement depended on device order (it now reads the
  ultrasonic sensor by slot), and m11's question was ambiguous (it now says what one move is and
  that the start cell does not count; the unit is "moves").
- **`cargo xtask package --ignore-rust-version`** passes `--ignore-rust-version` to every cargo
  command it starts (also `AXL_IGNORE_RUST_VERSION=1`). It is not the default; real machines
  have the pinned Rust. The package is built with `cargo rustc` so the rpath flag reaches only the
  final crate and the dependency builds stay reusable.
- **Accessibility.** UI scale 100 to 200%; every screen action is a button, and Alt+1 to Alt+7
  switch screens; status words accompany colours; the balance overlay has a tick or a cross.

## Spec problems found

1. SPEC.md gives the Home screen "Continue (last mission)" but does not say what Continue does on
   a first launch. AXL starts mission m01 and labels the button "Start mission m01".
2. The spec lists the Sandbox as a mode of the Simulate screen but not what it contains. It is a
   free-play floor with motor sliders (above).
3. The spec says the accuracy report ships "inside the app". A single-binary build cannot carry a
   file the validation run creates after compiling, so the report is a file beside the program
   (and under `target/validation/` in a checkout). About says so when it cannot find one.
4. The spec's Missions screen says "stars" per mission and "concept tags" but gives no chapter
   titles; the titles above are mine.
