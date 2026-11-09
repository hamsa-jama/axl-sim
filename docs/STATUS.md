# Status

One row per milestone from `SPEC.md`. States: not started, in progress, accepted, blocked.

| Milestone | State | Done | Left | How the acceptance checks were verified |
| --- | --- | --- | --- | --- |
| M0 Skeleton | accepted; tag `m00-done` created locally but not pushed (blocked, see `docs/decisions/0004-tag-push-blocked-in-sandbox.md`) | Workspace with all 11 crates and `xtask`; `rust-toolchain.toml`; `cargo xtask fetch-mujoco` (Linux run end to end, Windows and macOS paths written); CI workflow for 3 OSes; README and `docs/build.md`; `axl --headless --version`; empty Bevy/egui window. | Owner confirms CI is green on all 3 OSes and the window on a real GPU (see owner checks). | `axl --headless --version` prints `axl 0.1.0` (integration test `crates/axl-app/tests/headless_cli.rs`). Window: run under Xvfb with Mesa llvmpipe (software Vulkan), `axl --exit-after-frames 30` exits 0 after creating the window. CI: workflow written; see owner check O1. |
| M1 Physics core | not started | | | |
| M2 Rendering and grid | not started | | | |
| M3 Parts | not started | | | |
| M4 Builder | not started | | | |
| M5 Compiler | not started | | | |
| M6 Hardware | not started | | | |
| M7 Controllers | not started | | | |
| M8 Instruments | not started | | | |
| M9 Missions | not started | | | |
| M10 Release | not started | | | |

## Owner checks

Checks that need a GPU, a display or a person. Each has exact steps.

| Id | Milestone | Check | Steps |
| --- | --- | --- | --- |
| O1 | M0 | CI is green on Ubuntu, Windows and macOS | Open the Actions tab of `hamsa-jama/axl-sim`, find the latest run of `ci` on `main`, and confirm all three `check` jobs and the `wasm32-wasip1 target` job are green. The Windows (`tar.exe` unzip) and macOS (`hdiutil` mount) paths of `fetch-mujoco` have never run outside CI. |
| O0 | M0 | Push the `m00-done` tag (sandbox cannot) | `git fetch origin && git tag -a m00-done -m "M0 skeleton accepted" 80b4537 && git push origin m00-done`. Then M1 may start. |
| O2 | M0 | The window opens on a real GPU | On a desktop machine: `cargo xtask fetch-mujoco && cargo run -p axl-app --release`. A dark window titled "AXL" with a status bar reading "AXL 0.1.0" should open and close normally. |

## Development notes

- Sandbox build recipe (no `wasm32-wasip1`, Rust 1.97.0): see `docs/toolchains.md`.
- Software GPU for window smoke tests and later screenshots: `apt-get install xvfb mesa-vulkan-drivers vulkan-tools`, then `Xvfb :99 &` and `DISPLAY=:99 axl ...`. Mesa llvmpipe is picked automatically by wgpu.
