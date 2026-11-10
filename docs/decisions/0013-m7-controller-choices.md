# 0013: M7 controller choices

Status: accepted. Date: 2026-11-10.

## Context

M7 adds the wasmtime host, the C and Rust SDKs, the build pipeline and the Code screen. The spec
fixes the ABI and the watchdog; these are the choices it left open.

## Decisions

1. **The host knows nothing about the simulation.** `axl-host` talks to a `RobotApi` trait;
   `axl-sim` implements it (`SimRobot`) on the sim thread. The host tests use a recording mock, so
   every ABI function has a WAT test without MuJoCo.
2. **Reset restarts the program.** `Command::Reset` ends the running program (`axl_step` returns
   `AXL_MISSION_OVER`), resets physics and hardware, then starts the program again. A program that
   returns, or is stopped, leaves every motor braked and the simulation running by itself.
3. **Source lines come from DWARF.** The module is compiled with `WasmBacktraceDetails::Enable`;
   the first frame outside the SDK (`axl.h`, `axl-robot`, wasi-sysroot, rustlib, registry) is
   reported. C is built with `-g`, Rust with `debug = "line-tables-only"` in the template.
4. **A hand-written WASI shim** (`wasi_snapshot_preview1`): simulated clocks, seeded `random_get`,
   stdout and stderr to the log, no filesystem. Sleep and signals stop the program with an
   explanation; unknown WASI imports become traps so a missing feature is named, not silent.
5. **Toolchain detection is by looking, not by trusting.** wasi-sdk: `WASI_SDK_PATH`,
   `/opt/wasi-sdk`, `~/wasi-sdk`, `C:\wasi-sdk`, then `third_party/wasi-sdk` above the working
   folder (development). Rust: `cargo` plus `<sysroot>/lib/rustlib/wasm32-wasip1`.
6. **Project folders** are copies of `templates/<lang>/<mission>/` under
   `<data dir>/projects/<mission>/<c|rust>/`, never overwritten. The Rust copy's `axl-robot` path
   dependency is rewritten to the real `sdk/rust/axl-robot`. `AXL_ROOT` (or the working folder, or
   the executable's folder, or a parent) tells the app where `templates/` and `sdk/` are.
7. **Editing from outside.** The project is watched with `notify`; an outside edit reloads the
   editor when it has no unsaved changes, otherwise it offers "Load their version" or "Keep mine".
8. **CI builds the templates for real.** `AXL_REQUIRE_TOOLCHAINS=1` turns a missing wasi-sdk or
   `wasm32-wasip1` target into a test failure. In the sandbox the Rust template is only checked on
   the host (`cargo check`), and the Rust program test skips.

## Known gaps

- The Code screen runs the Rover in an empty world: missions and their worlds arrive with M8/M9, so
  the m01 wall is only in the tests, and plot, pose and drawing events are produced but not yet drawn.
- (Fixed after M7: `settings` is now gui-only, so `axl-app --no-default-features` builds again and CI checks it.)
