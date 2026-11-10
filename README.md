# AXL

AXL (pronounced "axle") is a desktop robotics simulator for teenagers. Learners build robots from interlocking brick parts, program them in C or Rust, and test them in realistic 3D on MuJoCo physics, the engine used in robotics research.

**Status:** version 1 is complete (milestones M0 to M10 are accepted); the owner checks before the `v1.0.0` tag are listed in [`docs/STATUS.md`](docs/STATUS.md). The guide for learners is [`docs/guide.md`](docs/guide.md). See [`docs/STATUS.md`](docs/STATUS.md) for progress by milestone.

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

## The screens

`axl` opens on Home. The bar at the top (or Alt+1 to Alt+7) switches between the seven screens:

| Screen | What it does |
| --- | --- |
| Home | Continue your last mission, Missions, Sandbox, Builder, Settings, overall progress |
| Missions | 12 missions by chapter with status, stars and ideas; a detail view with the brief, goal, rules, starter robot and exact success criteria |
| Builder | Build a robot from parts; Simulate runs it in the sandbox |
| Code | C or Rust editor, API reference, build log, and the mission's world on the stage |
| Simulate | 3D view with time controls, overlays, inspector, plotter; motor sliders in the sandbox |
| Results | Pass or fail per criterion, metrics, replay, hints; **Watch seed N in 3D** |
| Settings | Quality, units, grid opacity, UI scale (100 to 200%), invert orbit, toolchain paths, data folder; About shows the version and the accuracy report |

Command-line shortcuts: `--screen home|missions|builder|code|simulate|results|settings|about`, `--detail mNN` (with `--screen missions`), `--code mNN [--code-lang c|rust] [--code-run] [--grade robot.wasm]`, `--builder`, `--sandbox rover`, `--lookdev`, `--parts QUERY`. `axl --lookdev --screenshot out.png` renders to a PNG; `--stress-boxes N`, `--quality low|medium|high|ultra` and `--perf-seconds S` support the perf check in `docs/perf.md`. `axl --headless --version` prints the version, and `axl --headless --mission m03 --controller robot.wasm --seed 1` grades a program without a window.

## Release packages

`cargo xtask package` builds the release package for this OS. On a machine whose Rust is older than the pinned version, `cargo xtask package --ignore-rust-version` (or `AXL_IGNORE_RUST_VERSION=1`) passes that flag to every cargo command; it is not the default. A tag such as `v1.0.0` runs `.github/workflows/release.yml`, which builds the three packages, runs the accuracy suite and attaches the report to the release.

## Opening an unsigned macOS build

Version 1 macOS packages are not signed. After copying `AXL.app` to Applications, right-click it, choose Open, then Open again in the dialog. Tagged releases (`v*`) build the packages in GitHub Actions.

## Licence

MIT OR Apache-2.0.
