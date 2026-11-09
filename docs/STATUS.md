# Status

One row per milestone from `SPEC.md`. States: not started, in progress, accepted, blocked.

| Milestone | State | Done | Left | How the acceptance checks were verified |
| --- | --- | --- | --- | --- |
| M0 Skeleton | accepted; tag `m00-done` created locally but not pushed (blocked, see `docs/decisions/0004-tag-push-blocked-in-sandbox.md`) | Workspace with all 11 crates and `xtask`; `rust-toolchain.toml`; `cargo xtask fetch-mujoco` (Linux run end to end, Windows and macOS paths written); CI workflow for 3 OSes; README and `docs/build.md`; `axl --headless --version`; empty Bevy/egui window. | Owner confirms CI is green on all 3 OSes and the window on a real GPU (see owner checks). | `axl --headless --version` prints `axl 0.1.0` (integration test `crates/axl-app/tests/headless_cli.rs`). Window: run under Xvfb with Mesa llvmpipe (software Vulkan), `axl --exit-after-frames 30` exits 0 after creating the window. CI: workflow written; see owner check O1. |
| M1 Physics core | accepted | `axl-mujoco` safe API (load, step, state save/restore, rays with normals, contacts with forces, warnings, applied forces, inertia helpers); `axl-validate` harness and report writers; V01 to V06 and V15; `cargo xtask validate` and `mujoco-env`; CI runs the suite and uploads the report. | Nothing for M1. V07 to V14 and V16 to V18 belong to M5 and M6. | `cargo xtask validate`: V01 to V06 and V15 pass (e.g. impact 0.4511 s vs 0.4516 s expected, cylinder 0.5682 m vs 0.5676 m). 27 unit tests in `axl-mujoco` cover every public function. `crates/axl-validate/tests/no_unsafe.rs` shows no `unsafe` outside `axl-mujoco`. mujoco-rs 6.1.0+mj-3.12.0 resolves and links against the fetched 3.12.0 library, so it is kept. |
| M2 Rendering and grid | accepted; tag `m02-done` created locally, push command in O5 | `axl-render` crate: studio lighting (procedural cubemap, key light, AgX, physical exposure, bloom, saturation grade), part materials with micro-surface normal map, contact shadows, SSAO, cascaded shadows, quality presets Low to Ultra, first-launch benchmark with persisted settings, grid floor shader (Builder 60% and Simulate 25% opacity, spacing and distance fade, red X and green Y axes, stud dots, Studs/Metric toggle, measure tool), camera rig (orbit, pan, zoom, Orbit/Follow/Top-down, Home, F focus, invert orbit), egui control bar, status bar and axis gizmo, snapshot buffer with interpolation and body sync, `--lookdev`, `--screenshot`, `--stress-boxes`, `--quality`. Decision 0005 (WESL shader and procedural environment). | Low preset `render_scale` 0.75 is stored in the preset data but not applied (needs an offscreen upscale pass). Follow mode is implemented but was not exercised by a scripted run. Stud dots and the measure tool work on the floor only; part surfaces come with M4. Real-GPU frame rate and owner approval of the look (O4, O6). | `--lookdev --screenshot` under Xvfb + llvmpipe writes a PNG; viewed and committed under `docs/screenshots/` (High 1920x1080 plus metric, simulate, top-down, close-up, low, medium, ultra, far at 960x540). Grid fade, axes, unit toggle and measure readout checked with xdotool drives and screenshots. Stress scene (200 boxes) renders through the snapshot path (`perf:` line printed; llvmpipe gives about 9 fps, expected). First-launch benchmark picked Low on llvmpipe and wrote `settings.ron`. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (unit tests for grid math, quality, frames, materials, lighting, placeholder meshes, scene, sync, settings) and the `--no-default-features` build all pass. |
| M3 Parts | in progress (all checks run; waiting for the merge step below) | `axl-parts`: part types, RON loader, 75 part files, generators (profiles, earcut, extrusion with smooth bevels, lathe, block shells with hollow undersides and tubes, holes, involute gears, beams, axles, pins, wheels with tread, caster) and analytic volumes. `axl-render`: real part meshes, studio tent environment plus pluggable HDRI (`assets/render/studio.hdr`), part browser (`axl --parts all`, egui panel with mass, volume, connectors, collision shapes and colour switch), look-dev scene from real parts (`assets/scenes/lookdev.ron`). Decision 0007 (realism values). Post-fix real-GPU fixes from main: Low no longer logs `hdr true`, `WGPU_BACKEND=gl` exits with status 1. | Real-GPU approval of the look (O7). Low render scale 0.75, floor shader cost (see `docs/perf.md`). | `cargo test -p axl-parts`: all 75 files load, match the generator, every body mesh is closed and outward facing, computed masses equal volume x 1040 kg/m3, 5 sample volumes (brick 2x4, plate 2x8, beam 5, axle 8, gear 24) match hand calculations written in the test comments to within 1%. Browser and look-dev checked by screenshots under llvmpipe (`docs/screenshots/m3-*`). |
| M4 Builder | not started | | | |
| M5 Compiler | not started | | | |
| M6 Hardware | not started | | | |
| M7 Controllers | not started | | | |
| M8 Instruments | not started | | | |
| M9 Missions | not started | | | |
| M10 Release | not started | | | |

## Where I stopped

M2 is accepted and merged into `main` (merge commit 66135a5). M3 has not been started. Next: branch `milestone/m03` from `main`, then read `SPEC.md` "Part catalog" and write the part types, the RON loader and the 75 part files under `assets/parts/`, then procedural meshes (bevels, gear teeth, tyre tread), volume and mass, and the debug part browser.

## Owner checks

Checks that need a GPU, a display or a person. Each has exact steps.

| Id | Milestone | Check | Steps |
| --- | --- | --- | --- |
| O1 | M0 | CI is green on Ubuntu, Windows and macOS | Open the Actions tab of `hamsa-jama/axl-sim`, find the latest run of `ci` on `main`, and confirm all three `check` jobs and the `wasm32-wasip1 target` job are green. The Windows (`tar.exe` unzip) and macOS (`hdiutil` mount) paths of `fetch-mujoco` have never run outside CI. |
| O3 | M1 | Push the `m01-done` tag (sandbox push refused) | `git fetch origin && git tag -a m01-done -m "M1 physics core accepted" c8f2a82 && git push origin m01-done`. |
| O0 | M0 | Push the `m00-done` tag | Done by the owner (tag points at 80b4537). |
| O4 | M2 | 200 boxes hold 60 fps on High on a mid-range GPU | Follow `docs/perf.md`: `cargo run -p axl-app --release -- --stress-boxes 200 --quality high --perf-seconds 10`, read the `perf:` line (needs `fps` at or above 60 and `p99_ms` below about 20) and record it in the table in `docs/perf.md`. Also run once without `--quality` on a fresh data folder to confirm the first-launch benchmark picks a sensible preset. |
| O5 | M2 | Push the `m02-done` tag (sandbox push refused) | `git fetch origin && git tag -a m02-done -m "M2 rendering and grid accepted" 66135a5 && git push origin m02-done`. |
| O6 | M2 | Approve the look | Open `docs/screenshots/lookdev-high-1920x1080.png`, then on a real GPU run `cargo run -p axl-app --release -- --lookdev` and compare. Optional: swap the procedural studio cubemap for a Poly Haven HDRI (see decision 0005). |
| O2 | M0 | The window opens on a real GPU | On a desktop machine: `cargo xtask fetch-mujoco && cargo run -p axl-app --release`. A dark window titled "AXL" with a status bar reading "AXL 0.1.0" should open and close normally. |
| O7 | M3 | Approve the M3 look on a real GPU | Open `docs/screenshots/m3-lookdev-after-realism-high-1920x1080.png` and the two 1280x720 "after" images next to the "before" images, then on a real GPU run `cargo run -p axl-app --release -- --lookdev --quality high` and compare. Look for: softbox reflections on the red brick top and sides, highlights on bevels and stud rims, firm grounding. Known problem: SSAO noise (decision 0007). Also try `--parts all` and click a part. |
| O8 | M3 | Push the `m03-done` tag (sandbox push refused) | `git fetch origin && git tag -a m03-done -m "M3 parts accepted" <merge commit on main> && git push origin m03-done`. The merge commit hash is in the "Where I stopped" section. |
| O9 | M3 | Add a Poly Haven studio HDRI (optional, recommended) | 1. Open https://polyhaven.com/hdris/studio and pick a neutral indoor studio HDRI (CC0, for example "Studio Small 08"). 2. Download the 2K `.hdr` file. 3. Save it as `assets/render/studio.hdr` in the repository (it is loaded at start-up if present; a file that fails to load falls back to the procedural studio and logs why). 4. Run `cargo run -p axl-app --release -- --lookdev` and compare. 5. Commit the file only if the licence note is added to `assets/render/README`-style text in the commit message (CC0 needs no attribution). |

## Development notes

- Sandbox build recipe (no `wasm32-wasip1`, Rust 1.97.0): see `docs/toolchains.md`.
- Software GPU for window smoke tests and later screenshots: `apt-get install xvfb mesa-vulkan-drivers vulkan-tools`, then `Xvfb :99 &` and `DISPLAY=:99 axl ...`. Mesa llvmpipe is picked automatically by wgpu.

## Decisions that supersede the spec text

- `SPEC.md` says Rust "1.95 or newer". Decision record 0003 supersedes it: Bevy 0.20 needs 1.97.1, which `rust-toolchain.toml` pins.
- A milestone counts as accepted when its merge commit is on `main` and this file marks it accepted. Each `mNN-done` tag is created locally and pushed once; if the push is refused, the command goes in the owner checks above.
