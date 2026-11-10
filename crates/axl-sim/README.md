# axl-sim

The sim thread: lockstep loop, pacing and snapshots.

M5 has the sandbox: `Sandbox::spawn(&compiled, &volts)` starts a thread that steps MuJoCo at 1 ms
against the wall clock and publishes `SimSnapshot`s (every body pose) through a triple buffer;
`Command`s (pause, resume, reset, speed, volts, stop) go over a channel. `run_headless` runs without
pacing. Hardware models, the controller and the rewind buffer arrive in M6 and later.

Part of the AXL workspace. See `SPEC.md` for the design and `docs/architecture.md` for how the crates fit together.

M8 adds the instruments' sim side: `Step`, `Rewind` (deterministic replay from 0, with `Checkpoint`
events once per second carrying a joint-position hash), `Pick`, `ApplyDragForce`/`ClearDrag`,
a richer `SimSnapshot` (inspector data, hinges, contacts, sensor visuals, plot channels) and
`plot::PlotStore` with CSV export and parsing.
