# 0006: Fixes after the first run on real hardware

Date: 2026-11-09. Status: accepted.

## Context

The owner ran M2 on an Intel UHD 620 laptop (Vulkan, Mesa). One `--lookdev --screenshot` run saved an
empty image, the 200-box stress scene ran at 13.5 fps (High) and 22.3 fps (Low), and the first-launch
benchmark left no trace.

## What was found

- **Empty screenshot.** The capture ran after a fixed number of frames. Bevy compiles pipelines in
  the background, so on a slow first run the image could be captured before they finished. Captures,
  perf runs and the benchmark now wait until no pipeline has been waiting for 20 frames in a row,
  and log an ERROR if that never happens.
- **The environment filter ran every frame.** `GeneratedEnvironmentMapLight` in Bevy 0.20.1
  re-runs its downsample, radiance and irradiance passes every frame. The studio never changes, so
  once the filtered maps exist the generator component is removed and the `EnvironmentMapLight` it
  created keeps them.
- **Materials did not batch.** Per-part variation made nearly every part's material unique.
  The cache key now quantises to three levels each of roughness and brightness (the spec's +-0.03
  and +-1.5% remain the extremes).
- **Benchmark.** It was skipped by every scripted run (`--lookdev`, `--stress-boxes`, `--perf-seconds`,
  `--screenshot`, or `--quality`), which is what the owner ran. A normal first start does run it. It
  also measured with vsync on, which hides the difference between a fast and a slow GPU, so it now
  measures with vsync off, waits for steady state, logs its thresholds, result and the file it wrote,
  and gives up with a warning if frames stall. `AXL_DATA_DIR` points the settings at another folder and
  `--exit-after-benchmark` ends the run once it has chosen.
- `WGPU_BACKEND=gl` panics inside Bevy because the OpenGL backend is not compiled in. A panic hook now
  prints which backend was requested and what to do.
- The Low preset has no bloom, so it no longer needs a half-float (HDR) render target.

## Added

Start-up diagnostics, `--disable <list>` and `--safe-render`, `perf.json`, per-pass GPU timings and a
`docs/perf.md` frame budget. See `docs/perf.md`.

## Not changed

The Low preset's `render_scale` of 0.75 is still not applied. Bevy 0.20.1's
`MainPassResolutionOverride` exists only for upscalers such as DLSS; a general version needs an
offscreen render and a full-screen blit, which interacts with egui and picking. It is deferred until
owner measurements show it is needed.
