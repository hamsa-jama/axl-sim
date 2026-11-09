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
