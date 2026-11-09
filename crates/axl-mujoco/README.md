# axl-mujoco

The only crate allowed `unsafe` code, and a small safe layer over `mujoco-rs` 6.1.0 (MuJoCo 3.12.0). It currently needs none, because `mujoco-rs` already wraps everything AXL uses.

`Physics` owns one model and its data: load MJCF, step, read joints, bodies, sensors, rays (with surface normals), contacts with forces, apply forces, save and restore the full state, and read MuJoCo's warning counters. Only the sim thread may use it.

Build and test need `MUJOCO_DYNAMIC_LINK_DIR` and the library on the loader path: see `docs/build.md` and `cargo xtask mujoco-env`.
