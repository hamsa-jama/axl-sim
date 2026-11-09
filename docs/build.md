# Building AXL

## Prerequisites

- Rust, via [rustup](https://rustup.rs). `rust-toolchain.toml` selects the right compiler and adds the `wasm32-wasip1` target on first use.
- `curl` and `tar` on `PATH` (Windows 10+ and macOS ship both). On macOS, `hdiutil` is also used.
- Linux only: `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.

## Steps

```sh
git clone https://github.com/hamsa-jama/axl-sim
cd axl-sim
cargo xtask fetch-mujoco      # downloads and checks MuJoCo 3.12.0 into third_party/
cargo run -p axl-app --release
```

Useful commands:

| Command | Does |
| --- | --- |
| `cargo run -p axl-app --release` | Open AXL (needs a display and a GPU). |
| `cargo run -p axl-app --no-default-features -- --headless --version` | Headless binary without Bevy. Compiles in seconds. |
| `cargo test --workspace` | All unit and integration tests. |
| `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` | What CI checks first. |

## MuJoCo linking

The environment variables for linking `mujoco-rs` against the downloaded library are added here in milestone M1, once `axl-mujoco` exists.

## Windows and macOS notes

- Windows: `fetch-mujoco` unpacks the zip with the system `tar.exe` (bsdtar), not a GNU tar from Git.
- macOS: the release is a `.dmg`; `fetch-mujoco` mounts it with `hdiutil`, copies `mujoco.framework` out and unmounts it.
