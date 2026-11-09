# Architecture

See the "Architecture" section of `SPEC.md` for the thread model and message types. This file tracks what the code actually does; update it with each milestone.

## Crates today (after M0)

All crates exist as skeletons. Only `axl-app` has code: a clap command line, a headless mode that prints the version, and an optional Bevy window with an egui status bar.

| Crate | State |
| --- | --- |
| `axl-app` | M0: CLI, headless `--version`, empty window |
| `axl-render`, `axl-parts`, `axl-build`, `axl-compile`, `axl-mujoco`, `axl-hw`, `axl-host`, `axl-sim`, `axl-world`, `axl-missions`, `axl-validate` | Empty skeletons |
| `xtask` | `fetch-mujoco` |
