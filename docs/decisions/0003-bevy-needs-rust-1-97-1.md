# 0003: Bevy 0.20 needs Rust 1.97.1

Status: accepted (M0).

## Problem

Both Bevy 0.20.0 and 0.20.1 declare `rust-version = "1.97.1"`. The development sandbox has Rust 1.97.0 and cannot install another toolchain. The spec asks for "1.95 or newer", so 1.97.1 is allowed.

## Decision

`rust-toolchain.toml` pins 1.97.1 (CI and users get exactly that). In the sandbox, builds use `cargo ... --ignore-rust-version` with the installed 1.97.0 and `RUSTUP_TOOLCHAIN` set (see `docs/toolchains.md`). Bevy 0.20.1 is pinned with `=` in `crates/axl-app/Cargo.toml`.

## Risk

A real incompatibility between Bevy and 1.97.0 would show up only as a compile error; none has so far. CI uses the real 1.97.1 and is the final judge.
