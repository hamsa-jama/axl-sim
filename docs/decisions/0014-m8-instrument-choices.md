# 0014: M8 instrument choices

Status: accepted. Date: 2026-11-10.

## Context

M8 adds the time controls, rewind, the seven overlays, the inspector, the plotter and the mouse
spring. The spec fixes what they show; these are the choices it left open.

## Decisions

1. **Rewind is a replay from 0, never a state restore.** `Command::Rewind(t)` restarts the
   controller and physics, runs flat out (no pacing) to `t`, then pauses. Drag forces are logged by
   simulated millisecond and replayed; the log after `t` is dropped on arrival (it was the old
   future). Whole seconds send a `Checkpoint` event with a joint-position hash (FNV-1a of the `qpos`
   bits) and every body pose. The scrub bar previews the checkpoint at or before the cursor and only
   rewinds when released. The tests compare the replay's hash with the first run's at every second.
2. **One shared sim-thread loop.** The sandbox (no program) and a controller run use the same
   `sim_thread`, so time controls, picks and drags behave the same on both screens.
3. **The overlays read plain data.** The app copies each snapshot into `LiveView` and the
   controller's pose and line events into `Trails`; `axl_render::instruments` only draws gizmos from
   them, through `axl_to_bevy`. Two gizmo groups (solid, dashed) draw on top of the scene with a
   depth bias.
4. **Floor contacts are held for 70 ms.** A robot at rest micro-bounces and some snapshots have no
   contact at all; the support polygon would flicker. The hold is applied in the app, not the
   physics.
5. **Colour is never the only signal.** The balance overlay draws a tick (centre of mass inside the
   support polygon) or a cross (outside) next to the green or red line.
6. **X-ray changes the cached part materials** (30% alpha, blended) and restores them when
   switched off, instead of swapping materials per entity.
7. **The mouse spring is computed in the app and held per frame.** Force = `m w^2 (target - grab) -
   2 z m w v` on the body under the cursor, with `w = 9 /s`, `z = 0.7` (interface feel, not physics)
   and a cap of 1.5 robot weights. A force that is only refreshed once per frame must be slow
   compared with the frame time, so `w` is lowered to `0.5 / frame time` on a slow frame (found
   under llvmpipe at 3 fps, where the fixed spring threw the robot and MuJoCo kept resetting).
   Left press picks (Alt + left orbits); `ClearDrag` runs when the button is released.
8. **Cursor rays and labels correct for the shifted projection.** The Code screen shifts the
   projection to the middle of the free area (`sub_camera_view`), which Bevy's main-world
   `viewport_to_world` and `world_to_viewport` do not know about; both are corrected by the same
   offset.
9. **Plots.** `egui_plot` 0.37.0 (the version that pairs with egui 0.36), 50 samples per second of
   simulated time for the built-in channels, 300 s of history, a 5 to 60 s window, the cursor
   readout, and CSV through `rfd` (`AXL_CSV_EXPORT_PATH` writes without a dialog, for tests). The
   CSV has a `time_s` column then one column per channel, RFC 4180 quoting, empty cells where a
   channel has no point at that time.
10. **Per-material cost on Low.** One material per colour on Low (see `docs/perf.md`).
11. **Scripted runs.** The hidden `--instruments` flag (`overlays=1+3,plots,select=N,pause_at=S,
    drag_at=S,rewind=AT:TO,export_at=S,hover_joint`) drives the screenshots under Xvfb.

## Known gaps

- Built-in plot channels are sampled once per rendered frame (so about 60 per second on a real GPU,
  3 under llvmpipe); controller `axl_plot` points arrive at the controller's rate.
- The joint hover picks the hinge anchor nearest to the cursor in screen space (16 px), not the
  hinge's own geometry.
