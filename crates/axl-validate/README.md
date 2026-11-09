# axl-validate

The accuracy suite. Each test compares the physics with a textbook answer, computing the expected value from the model's own parameters.

- Implemented: V01 to V06 and V15 (milestone M1). V07 to V14 and V16 to V18 arrive with M5 and M6.
- Run: `cargo xtask validate`, or `cargo run -p axl-validate --release -- [--list] [--out DIR] [V01 ...]`.
- Output: `report.md` and `report.json` (JSON is written by hand; no JSON crate is on the approved list).
- `tests/no_unsafe.rs` enforces that `unsafe` appears only in `axl-mujoco`.
