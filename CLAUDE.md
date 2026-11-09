# AXL: instructions for coding agents

`SPEC.md` is the authoritative design and development spec. Read all of it before writing any code. If this file and `SPEC.md` disagree, `SPEC.md` wins, except for the sandbox notes at the end of this file.

## How to work

- Build the milestones in `SPEC.md` strictly in order (M0, then M1, and so on).
- Track progress in `docs/STATUS.md`. Keep one row per milestone with its state (`not started`, `in progress`, `accepted`, `blocked`), what is done, what is left, and how each acceptance check was verified.
- For each milestone:
  1. Branch `milestone/mNN` from `main`.
  2. Make small commits with conventional messages, for example `feat(compile): merge coaxial hinge links`.
  3. Before every commit, run `cargo fmt`, then `cargo clippy` with `-D warnings`, then the tests for the crates you touched.
  4. When every acceptance check passes, merge into `main` with `--no-ff` and tag `mNN-done`. Checks that need a GPU or a human go in `docs/STATUS.md` as "owner check", with exact steps for the owner.
  5. Push `main`, the tag and the branch. Run `git fetch origin main` before pushing.
- Languages: Rust everywhere, WGSL for GPU shaders, and C only for the learner SDK (`sdk/c/`), C templates and C reference solutions. No Python, JavaScript or shell scripts in the repo. GitHub Actions YAML is fine.
- Before calling any API of Bevy 0.20, bevy_egui 0.43, wasmtime 50 or mujoco-rs 6.1, read that exact version's source and examples in `~/.cargo/registry/src/*/<crate>-<version>/`. Never guess an API from memory: these versions are newer than most training data.
- When blocked, write a decision record in `docs/decisions/` and stop. Do not skip ahead.

## Sandbox notes

These describe the cloud environment that agents run in, not end-user machines.

- **Rust.** Rust 1.97 stable is installed.
- **No `wasm32-wasip1` Rust target.** `rustup target add wasm32-wasip1` fails here because static.rust-lang.org is blocked, so Rust controllers cannot be compiled to WebAssembly in this sandbox. Check the Rust SDK and templates with `cargo check` for the host target, and leave the `wasm32-wasip1` build to CI, which has normal internet access. CI must install the target itself.
- **C controllers.** Download wasi-sdk from GitHub releases. For example, `https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-27/wasi-sdk-27.0-x86_64-linux.tar.gz` works; newer versions may exist, so try 28, 29 and so on. Extract it to `third_party/`, which is git-ignored, and pin the version you used in `docs/toolchains.md`.
- **Network.** MuJoCo release archives download from github.com releases, and crates.io works. Most other hosts, including api.github.com and static.rust-lang.org, are blocked.
- **No GPU and no display.** You can compile the Bevy app but not open a window. Use headless mode for tests.
- **2 CPU cores and 7 GB RAM.** Use `CARGO_BUILD_JOBS=2`. While iterating, prefer `cargo check` and `cargo test -p <crate>` over full workspace builds. Turn off Bevy default features you do not need (audio, for example), and use `debug = "line-tables-only"` in the dev profile to save memory.
