# axl-build

The robot assembly: placed parts on a 0.1 mm grid with 24 orientations, connections derived from geometry (never stored), snapping, validation, a command API with 200-step undo and redo, `.axlbot` files and the two starter robots.

`Builder` is the one type the Builder screen and the tests drive. `cargo run -p axl-build --example make_starters` regenerates `assets/robots/*.axlbot` from the recipes in `starter.rs` (a test checks they still match).

Part of the AXL workspace. See `SPEC.md` for the design and `docs/architecture.md` for how the crates fit together.
