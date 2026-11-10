# 0015: M9 mission choices

Status: accepted. Date: 2026-11-10.

## Context

M9 adds the twelve missions, the graded headless runner, the reference solutions and the learning
design. The spec fixes the mission list and the grading rules; these are the choices it left open,
and the places where the spec had to be extended.

## Decisions

1. **`MatDef` has a `center` field** (`[f64; 2]`, default the origin). The spec's mats do not all
   start at the origin: the line track and the test track are laid out around a point, and the
   ramp lab has its mat beside the ramp. Existing world files keep working because of the default.
2. **The `Criterion` enum is wider than the spec's list.** Added: `FinalPose`, `FrontStop`,
   `TurnTo` (uses the point midway between the drive wheels, the `pivot` sample, for its drift
   limit), `Waypoints`, `FrontClearance`, `HeadingLock`, `BelievedPose`, `EndNearStart`,
   `UsedPlot`, `MaxParts`, `FinishWithin`, `NoTip`, `NoContact`, `MeanSpeed` and `Report` with an
   `Expected::Measured` value. Each is unit-tested on synthetic paths
   (`crates/axl-missions/src/eval/tests.rs`). `results::describe` words every criterion for the
   brief.
3. **Walls are 12 mm thick and 100 mm high** in every world (a 2x4 brick wall is about 16 mm; 12 mm
   is a printed or painted board). It is the `thickness` field of each wall and of the maze.
4. **Dimensions.** The walled arena is 1.5 m x 1.5 m (walls on the +-0.75 m square). The maze is
   6 x 6 cells of 0.25 m, so 1.5 m across, with 100 mm walls. The maze layouts are generated from
   `layout_seeds: [11, 22, 13]`; these three seeds were chosen so that the right-hand-rule route
   is at most 40 moves, which fits the 120 s limit with a trapezoid drive (the first three
   candidate seeds needed 60 or more moves).
5. **The Rover starter changed.** The hub moved to x = -16 mm and the caster to x = -40 mm. With
   the caster close to the axle the drive wheels lost grip on the m07 ramp (wheel load fraction
   too low, the tyres spun). Both starters (`rover`, `rover_tof`) and their `.axlbot` files and
   the MJCF snapshot were regenerated with `cargo run -p axl-build --example make_starters`.
6. **m12 needs a designed robot.** `missions/solutions/m12.axlbot` (the "climber", 12 parts) puts
   the hub over the drive axle, the caster 48 mm behind it, and a short plate so its nose clears
   the foot of the ramp. The runner takes it with `--robot`. In the app, build missions are graded
   with the newest design in the robots folder.
7. **Pushes act 50 mm ahead of the centre of mass** (m10's disturbance), so that they turn the
   robot as a real bump would, not only shove it.
8. **FSRS-4.5 default weights.** The spec says FSRS-4.5 with "the published parameters"; the 17
   default weights in `learn/fsrs.rs` are the FSRS-4.5 defaults. The spec does not say whether
   other parameters (retention target) are fixed, so the target retention is 0.9 (interval equal
   to stability). **Spec problem:** please confirm both.
9. **Hints unlock after the first failed run and open in order.** Opening any hint loses the
   "no hints" star. The prediction is required before the first submit when the mission has one,
   and the real answer comes from the runner's own measurement (`RunTrace::measured`, or the final
   x or y).
10. **Reference solutions.** C is built with wasi-sdk 27 and graded here on every seed. The Rust
    ports are a separate workspace (`missions/solutions/rust`) built for `wasm32-wasip1` by
    `cargo xtask sdk-check` where that target exists; in this sandbox they only get `cargo check`
    on the host, so CI (`--require-rust`) is what grades them.
