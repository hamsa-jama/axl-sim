# Toolchains

| Tool | Pinned version | Where it is pinned | Notes |
| --- | --- | --- | --- |
| Rust | 1.97.1 | `rust-toolchain.toml` | Bevy 0.20.x declares `rust-version = 1.97.1`. Components `rustfmt` and `clippy`; target `wasm32-wasip1`. |
| MuJoCo | 3.12.0 | `xtask/src/mujoco_checksums.rs` | `cargo xtask fetch-mujoco`. |
| wasi-sdk | 27 (clang for `wasm32-wasip1`) | `WASI_SDK_VERSION` in `.github/workflows/ci.yml` | Download URL form: `https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-27/wasi-sdk-27.0-x86_64-linux.tar.gz`. Versions 28 to 34 also exist; do not move without testing the C templates (milestone M7). Extract to `third_party/wasi-sdk` and set `WASI_SDK_PATH`. |

## Development sandbox exceptions

The cloud sandbox used to develop AXL has Rust 1.97.0 and cannot install the `wasm32-wasip1` target (`static.rust-lang.org` is blocked), so `rust-toolchain.toml` would make every `cargo` call fail there. In the sandbox:

```sh
export RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu   # ignore rust-toolchain.toml
cargo check --ignore-rust-version ...                      # Bevy wants 1.97.1
```

CI and end-user machines use the pinned toolchain and need neither workaround.
