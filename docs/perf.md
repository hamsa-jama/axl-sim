# Performance checks

The spec sets frame-rate targets that only a real GPU can confirm. The sandbox that built the code
has no GPU (Mesa llvmpipe renders in software, around 1 frame per second at 1080p), so the numbers
below are **owner checks**. Fill in the results table when you run them.

## How to measure

`axl` has a built-in frame-time recorder for these checks. It starts after one second (shader
compile and environment filtering), runs for the number of seconds you give it, prints one line and
exits. Vsync is switched off for these runs so the monitor refresh rate does not cap the result.

```sh
cargo xtask fetch-mujoco                       # once
cargo run -p axl-app --release -- --stress-boxes 200 --quality high --perf-seconds 20
```

Output: `perf: preset=High frames=... avg_ms=... fps=... p95_ms=... p99_ms=...`

The stress scene is 200 chamfered boxes moving through the same snapshot-interpolation path the
simulation will use, on the grid floor with shadows, contact shadows, ambient occlusion and TAA.
It is rendered at 1920x1080.

## M2 acceptance: "200 boxes hold 60 fps on High on a mid-range GPU"

Pass when `fps` is at least 60 and `p99_ms` is below about 25 on a mid-range discrete GPU.

| Date | Machine and GPU | Preset | avg fps | p95 ms | p99 ms | Pass |
| --- | --- | --- | --- | --- | --- | --- |
| (not run yet) | | High | | | | |

## Other checks to run once

- Low preset on integrated graphics should reach 30 fps: `--quality low --stress-boxes 200 --perf-seconds 20`.
- First-launch benchmark: delete `settings.ron` from the data folder (see `docs/build.md`), start
  `axl`, and note which preset it picks. The cut-offs in `assets/render/quality.ron` are
  `placeholder: calibrate`.
