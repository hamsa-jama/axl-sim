# AXL

AXL (pronounced "axle") is a desktop robotics simulator for teenagers. Learners build robots from interlocking brick parts, program them in C or Rust, and test them in realistic 3D on MuJoCo physics, the engine used in robotics research.

**Status:** version 1 is being finished (milestones M0 to M9 are accepted). The guide for learners is [`docs/guide.md`](docs/guide.md). See [`docs/STATUS.md`](docs/STATUS.md) for progress by milestone.

- [`SPEC.md`](SPEC.md) is the full design and development specification.
- [`CLAUDE.md`](CLAUDE.md) holds working rules for coding agents.
- [`docs/build.md`](docs/build.md) has the full build notes.

## Build and run

You need [rustup](https://rustup.rs), `curl` and `tar`. On Linux, also install `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.

```sh
git clone https://github.com/hamsa-jama/axl-sim
cd axl-sim
cargo xtask fetch-mujoco          # MuJoCo 3.12.0 into third_party/, checksum verified
cargo run -p axl-app --release    # opens the AXL window (needs a display and a GPU)
```

Check every template and reference solution against its mission, headless (needs wasi-sdk, see [`docs/toolchains.md`](docs/toolchains.md)):

```sh
cargo xtask sdk-check
```

Build the release package for this OS into `target/package/` (Linux `.tar.gz`, Windows `.zip`, macOS `.dmg`):

```sh
cargo xtask package
```

Without a window, for graders and CI:

```sh
cargo run -p axl-app --release -- --headless --version
```

## What works today

- Workspace with every crate as a skeleton, pinned toolchain, CI on Linux, Windows and macOS.
- `cargo xtask fetch-mujoco`.
- `axl` opens the studio view with the grid floor, camera controls and a control bar (the first launch runs a short quality benchmark). `axl --lookdev --screenshot out.png` renders the look-dev scene to a PNG; `--stress-boxes N`, `--quality low|medium|high|ultra` and `--perf-seconds S` support the perf check in `docs/perf.md`; `axl --headless --version` prints the version.

Everything else is on the milestone list in `SPEC.md`.

## Opening an unsigned macOS build

Version 1 macOS packages are not signed. After copying `AXL.app` to Applications, right-click it, choose Open, then Open again in the dialog. Tagged releases (`v*`) build the packages in GitHub Actions.

## Licence

MIT OR Apache-2.0.
