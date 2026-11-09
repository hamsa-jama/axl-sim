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
| `cargo test --workspace` | All unit and integration tests (set the MuJoCo variables first). |
| `cargo xtask validate` | The accuracy suite; writes `target/validation/report.md` and `report.json`. |
| `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` | What CI checks first. |

## MuJoCo linking

`mujoco-rs` 6.1.0 links the shared library found through `MUJOCO_DYNAMIC_LINK_DIR` (an absolute path to the folder holding the library). The loader also needs to find the library at run time. `cargo xtask mujoco-env` prints both settings for this OS:

```sh
# Linux and macOS (bash):
while IFS= read -r line; do export "$line"; done < <(cargo xtask mujoco-env)
# CI appends the same lines to $GITHUB_ENV.
```

| OS | Link folder | Run-time search path |
| --- | --- | --- |
| Linux | `third_party/mujoco/3.12.0/linux/lib` | `LD_LIBRARY_PATH` |
| Windows | `third_party/mujoco/3.12.0/windows/lib` (`mujoco.lib`) | `PATH` includes the `bin` folder (`mujoco.dll`) |
| macOS | folder of `libmujoco*.dylib` inside `mujoco.framework` (found by xtask) | `DYLD_LIBRARY_PATH` |

`cargo xtask validate` sets these itself. Release packages put the library next to the executable (milestone M10). The macOS lookup has not been run outside CI.

## Windows and macOS notes

- Windows: `fetch-mujoco` unpacks the zip with the system `tar.exe` (bsdtar), not a GNU tar from Git.
- macOS: the release is a `.dmg`; `fetch-mujoco` mounts it with `hdiutil`, copies `mujoco.framework` out and unmounts it.
