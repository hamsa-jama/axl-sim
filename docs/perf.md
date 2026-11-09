# Performance checks

The spec sets frame-rate targets that only a real GPU can confirm. The sandbox that built the code
has no GPU (Mesa llvmpipe renders in software, a few frames per second), so its numbers say nothing
about real hardware. The tables below hold owner measurements.

## Reference machines

| Name | Machine | GPU | Notes |
| --- | --- | --- | --- |
| laptop-uhd620 | Arch Linux, kernel 7.2.14, Wayland | Intel UHD Graphics 620 (KBL GT2, 8086:5917), i915, Mesa 26.2.6, Vulkan | Only GPU in the machine. Integrated. This is the reference for the Low preset. |

## How to measure

`axl --perf-seconds N` records frame times in steady state, prints a summary and writes `perf.json`
in the current folder. Vsync is off for these runs. The run waits until no render pipeline has been
compiling for 20 frames in a row, so shader compilation never counts as a slow frame.

```sh
cargo xtask fetch-mujoco                       # once
cargo run -p axl-app --release -- --stress-boxes 200 --quality high --perf-seconds 10
```

The output has these lines:

- `perf: preset=... avg_ms=... fps=... p95_ms=... p99_ms=...` (the line older notes quote).
- `perf: drawn=... unique_meshes=... unique_materials=...`: how many draws can batch. Batches are
  at most meshes times materials.
- `perf: adapter=... backend=... present_mode=... resolution=...`.
- `perf: cpu main_ms=... render_ms=...`: main-thread and render-thread CPU time per frame.
- `perf: gpu <ms> <pass>`: GPU time per render pass, from GPU timestamp queries. They need Vulkan or
  DX12 and a driver that supports them (Intel ANV does). Without them the run says so and reports CPU
  times only.

`perf.json` has the same data (frame mean, p50, p95, p99, per-pass GPU and CPU times).

Start-up logs show the adapter that was chosen and every adapter that was available, the texture
formats in use, the resolution, present mode, the quality preset with the reason for it, and which
effects are disabled. Run with `RUST_LOG=info` to see them. Shader and pipeline errors are logged at
ERROR level by Bevy, and AXL adds one summary line when any pipeline failed to compile.

## Frame budget

A starting point, to be checked against owner measurements. Do not treat it as a contract.

60 fps is 16.7 ms per frame. A starting budget for the High preset on a mid-range discrete GPU:

| Pass | Budget (ms) |
| --- | --- |
| Shadow cascades | 3 |
| SSAO | 2 |
| Contact shadows | 1.5 |
| TAA | 1 |
| Main pass | 6 |
| egui | 1 |
| Everything else (prepass, bloom, tonemapping, upscaling) | the remaining 2.2 |

On the UHD 620, High is not expected to reach 60 fps. The spec asks for 30 fps on Low there.

## Finding the culprit on a machine

`--disable` takes a comma-separated list of effects to switch off: `grid` (the custom floor shader),
`env` (the studio environment map), `ssao`, `taa`, `contact-shadows`, `shadows`, `bloom`,
`tonemap`, `micro-normal` and `picking`. `--safe-render` switches them all off. Compare `perf:` lines
with one effect off at a time; the per-pass GPU times name the expensive pass directly.

## Fixes made after the first laptop run (decision record 0006)

Measured on laptop-uhd620 with `--stress-boxes 200 --perf-seconds 10` before the fixes: High 13.5 fps
(p99 about 100 ms), Low 22.3 fps (p99 58 ms).

| Change | Why | Effect |
| --- | --- | --- |
| The environment map is filtered once, not every frame | Bevy 0.20.1 runs the radiance and irradiance convolution of `GeneratedEnvironmentMapLight` every frame. | Remove those two passes from every frame (about 200 ms of the first llvmpipe frame; the llvmpipe run showed them as the two most expensive passes). Owner measurement pending. |
| Part variation uses 3 roughness and 3 brightness levels instead of about 200 | Each distinct material is a separate draw in every pass; 200 boxes had 200 materials. | `unique_materials` for 50 boxes went from 50 to 34, and for 200 boxes to at most 81 (9 colours x 9). Owner measurement pending. |
| Low preset renders to an 8-bit target (no HDR) | Bloom is off on Low, so the half-float target only costs memory traffic. | Owner measurement pending. |
| The first-launch benchmark measures with vsync off | With vsync every frame is at least 16.7 ms, so a fast GPU could not reach High or Ultra. | Correctness. |

Not done yet: the Low preset's `render_scale` of 0.75 is stored but not applied (it needs an
offscreen render and upscale pass). Measure first; see the owner checks.

## M2 acceptance: "200 boxes hold 60 fps on High on a mid-range GPU"

Pass when `fps` is at least 60 and `p99_ms` is below about 25 on a mid-range discrete GPU.

| Date | Machine and GPU | Preset | avg fps | p95 ms | p99 ms | Pass |
| --- | --- | --- | --- | --- | --- | --- |
| before fixes | laptop-uhd620 | High | 13.5 | | about 100 | not applicable (integrated GPU) |
| before fixes | laptop-uhd620 | Low | 22.3 | | 58 | no (target 30) |
| after fixes (main 1d4a26a, 1920x1000, 200 boxes, 10 s) | laptop-uhd620 | High | 14.1 | | 85.5 | below target on an integrated GPU; expected, High targets a discrete GPU |
| after fixes (same run) | laptop-uhd620 | Medium | 22.7 | | 47.6 | below target on an integrated GPU; expected, Medium targets an entry discrete GPU |
| after fixes (same run) | laptop-uhd620 | Low | 40.6 | | 27.5 | pass (target 30 fps) |
| (not run yet) | mid-range discrete | High | | | | |

## Other checks to run once

- Low preset on integrated graphics should reach 30 fps: `--quality low --stress-boxes 200 --perf-seconds 10`.
- First-launch benchmark from an empty data folder:
  `AXL_DATA_DIR=/tmp/axl-empty cargo run -p axl-app --release -- --exit-after-benchmark`, then read
  `/tmp/axl-empty/settings.ron`. The cut-offs in `assets/render/quality.ron` are `placeholder: calibrate`.

## Post-fix measurement on the reference laptop

laptop-uhd620, main at 1d4a26a, `--stress-boxes 200 --perf-seconds 10`, 1920x1000. Biggest GPU passes (ms):

| Preset | fps | p99 ms | opaque | SSAO | TAA | bloom | SMAA | FXAA |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| High | 14.1 | 85.5 | 38.5 | 13.5 | 7.3 | 3.8 | | |
| Medium | 22.7 | 47.6 | 20.8 | 9.4 | | 4.1 | 2.3 | |
| Low | 40.6 | 27.5 | 20.7 | | | | | 1.6 |

Low passes. High and Medium are below their targets on this integrated GPU, which the spec expects.
First launch from an empty folder logged "76.3 ms per frame (13.1 fps), chose Low" and wrote
`settings.ron`. The environment line reads "filtered once and frozen". The opaque pass is the largest
single cost on every preset (about 20.7 ms on Low and Medium); attribution runs with `--disable` are
pending from the owner.

## Review of opaque-pass cost (code reading, no changes made)

The owner measured about 20.7 ms in the opaque pass on both Low and Medium, so it does not depend on
the effects those presets differ in. Notes from reading the code; none of these is a measured cause.
`--disable grid` and `--disable shadows` runs on Low will say which matters.

- **Floor fragment cost.** The 200 m floor covers most of the screen, so most fragments in the opaque
  pass are floor fragments, each running the full `StandardMaterial` PBR path (environment map
  sampling, shadow lookup, directional light) plus `grid_floor.wesl`.
- **Grid shader per fragment.** Three `family()` calls (minor, major, bold), each doing its own
  `fwidth` of a vec2 and a `fract`/`abs`; one more `fwidth` for the axes; `distance()` for the fade;
  stud dots and measure markers sit behind uniform branches and cost nothing when off. That is
  roughly 4 derivative pairs and a few dozen ALU operations: small next to a PBR fragment, but
  it is paid by every floor pixel. Possible savings if the attribution run points here: compute
  `fwidth(uv)` once and scale it by 1/spacing (the three families differ only by a constant factor),
  and skip a family when its `spacing_fade` is already 0 or the distance fade is 0.
- **Shadows on Low.** One cascade, 1024 px, `Hardware2x2` filtering: one shadow-map lookup per
  fragment. This is already the cheapest setting; the cascade count is not the cost on Low. Medium
  uses 2 cascades with Gaussian filtering only when `soft`, which Medium does not set.
- **Fill rate.** At 1920x1000 the pass touches about 1.9 million fragments, plus depth pre-pass
  overdraw from the floor under every box. The render-scale 0.75 on Low would cut fragments by 44%
  if it were applied (decision 0006 defers it).

## Stress-scene materials, HDR on Low, render scale (M3 follow-ups)

- **Materials in the stress scene.** The 200 boxes use 9 colours and the quantised per-part variation
  (3 roughness x 3 brightness levels). That is at most 9 materials per colour and 81 overall; the
  owner's `unique_materials=69` is a normal draw from that set, so the scene already batches the way
  real robots will. `scene::quantised_variation` has a unit test that no uid gives more than 9 variations.
- **`hdr true` on Low.** The camera was spawned with the `Hdr` component, and the start-up log read it
  before the preset (applied through deferred commands) removed it, so the log lied for a few frames.
  `Hdr` is now added only by the preset, and the log waits 4 frames. Measured under llvmpipe the Low log
  line now reads `hdr false`.
- **Low render scale 0.75 is still not applied.** Read of Bevy 0.20.1: `MainPassResolutionOverride` only
  shrinks the main pass viewport inside a full-size texture and "does not affect post processing", so the
  built-in tonemapping, FXAA and upscaling passes would still read the full texture; only DLSS-style
  upscalers resample it. A general version needs the camera to render into an image of 0.75 size and a
  second camera to blit it to the window with egui on the second camera, and the cursor, picking and
  floor ray code to map window positions into the smaller target. That touches the camera input, picking
  and egui, and cannot be verified without a real GPU. It is left for after the owner's `--disable`
  attribution shows whether fill rate is the problem.

## Environment light (branch fix/env-lighting)

llvmpipe (software Vulkan, 256-bit), `--stress-boxes 200 --quality low --perf-seconds 10 --window-size 960x540`.
The numbers are only comparable with each other, not with a laptop GPU.

| Run | avg ms | p99 ms | main_opaque_pass_3d ms |
| --- | --- | --- | --- |
| before, env on | 95.3 | 110.4 | 61.7 |
| before, `--disable env` | 79.2 | 93.2 | 44.4 |
| after, env on | 96.2 | 124.6 | 47.7 |
| after, `--disable env` | 80.2 | 92.2 | 41.8 |

The opaque pass is where the environment is sampled. Its env share dropped from about 17 ms to about 6 ms
under llvmpipe. Frame averages did not move (the run-to-run spread under llvmpipe is several ms), so this
sandbox cannot confirm the laptop target of 2 ms. Owner check: re-run the owner's table on the laptop.
Per-pass lines (`perf: gpu  NN ms  render/...`) print again, sorted by cost, top-level passes only.

## Where the environment-light cost comes from (Bevy 0.20.1 source reading) and `--env-mode`

Laptop result that prompted this (UHD 620, Mesa ANV, Low preset): `--disable env` takes the opaque
pass from 20.6 ms to 10.6 ms, and decision 0008 (smaller maps) did not change it. A 256-square cube
cannot cost 10 ms at 1.9 Mpx by texture bandwidth alone, so the shader path is the suspect.

What the shader does (`bevy_pbr-0.20.1/src/light_probe/environment_map.wesl`, selected by
`mesh_view_bindings.wesl` and `render/mesh.rs`):

- **Array path** (`MULTIPLE_LIGHT_PROBES_IN_ARRAY`). Chosen when the device has
  `TEXTURE_BINDING_ARRAY` and `SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` and the
  array limits are at least 24 (`binding_arrays_are_usable`, `light_probe/mod.rs`). Per fragment it
  runs a loop (`light_probe_iterator_next`) over the clustered light-probe list, reads the cube
  array with a non-uniform index (`binding_array<texture_cube, 8>`), calls `textureNumLevels`, and
  samples diffuse and specular maps. The loop runs once more until it finds no probe and falls back to
  the view's probe.
- **Single path** (no define). No loop and no cluster lookup: one plain `texture_cube` pair for the
  view, a uniform for the mip count.
- **Clearcoat** (`STANDARD_MATERIAL_CLEARCOAT`) repeats the entire `compute_radiances` call for the
  coat layer: a second probe loop, a second specular and a second diffuse sample. AXL's ABS material
  has a clearcoat, so every plastic fragment pays the lookup twice.
- Both maps are `Rgba16Float`, the specular one with a full mip chain.

Which path each adapter takes, as measured with the new start-up log line
`environment light path: ...`:

- **llvmpipe (this sandbox): ARRAY.** The lead that llvmpipe lacks binding arrays is wrong: it reports
  both features and array limits of 1,000,000, so it takes the same path as the laptop should.
  Its cost drop with env off (17 to 6 ms) is therefore a software-renderer figure, not evidence
  about the array path.
- **Laptop UHD 620 (Mesa ANV, Gen9): unknown until the owner runs it.** The log line states it. If
  ANV advertises the features it takes ARRAY too. Non-uniform indexed descriptor arrays are known to
  be slow on Gen9 hardware, which is the leading suspect.

`--env-mode` (since decision 0011 the default is `single`; the others are diagnostics):

| Mode | What it does |
| --- | --- |
| `array` | The path the adapter allows (the old default). |
| `single` | Default. Masks `TEXTURE_BINDING_ARRAY` in `WgpuSettings.disabled_features`, so Bevy compiles the single-cube path. |
| `diffuse-only` | Keeps the diffuse map, replaces the specular map by a 1x1 black cube (the shader still samples, but from one texel). |
| `specular-only` | The reverse. |
| `ambient` | No environment map, flat ambient light (same as `--disable env`). |
| `no-clearcoat` | Normal environment path but every material's clearcoat is zeroed, to measure the doubled lookup. |

Laptop commands (read the `perf:` line and the GPU pass lines; the log line
`environment light path:` and `render: ... env-mode` tell which path ran):

```sh
for m in array single diffuse-only specular-only no-clearcoat ambient; do
  echo "== $m"
  cargo run -p axl-app --release -- --lookdev --quality low --env-mode $m --perf-seconds 10 2>&1 \
    | grep -E "environment light path|env-mode|perf:|opaque|GPU"
done
```

Read the result like this: if `single` is much cheaper than `array`, the cost is the binding-array
and probe-iterator path (fix: force the single path when the adapter is slow, or at least on Low).
If `no-clearcoat` is about half of the env cost, the clearcoat's second lookup matters (fix: drop the
clearcoat on Low and Medium). If only `ambient` is cheap, the cost is the cube sampling itself
(fix: smaller or `Rg11b10Ufloat` maps, a custom baked map).

## Environment-light cost: owner measurement and the `single` default (decision 0011)

Laptop UHD 620 (main at 211cde1), `--lookdev --quality low --perf-seconds 10`:

| mode | fps | p99 ms | opaque ms | GPU total ms |
| --- | --- | --- | --- | --- |
| array (old default) | 37.8 | 28.6 | 22.5 | 25.3 |
| single | 52.6 | 20.2 | 14.1 | 17.1 |
| diffuse-only | 38.4 | 27.8 | 22.0 | 24.8 |
| specular-only | 37.7 | 28.6 | 22.6 | 25.4 |
| no-clearcoat | 40.2 | 26.5 | 20.9 | 23.8 |
| ambient | 63.6 | 17.2 | 11.4 | 14.1 |

The cost is the binding-array, clustered light-probe path itself. `single` is now the default for
every preset.

What masking `TEXTURE_BINDING_ARRAY` switches off in Bevy 0.20.1 (source reading,
`bevy_pbr::light_probe::binding_arrays_are_usable` and `bevy_render_macros::as_bind_group`):
reflection probes in arrays and more than one irradiance volume (AXL uses neither: one environment
map on the camera), bindless lightmaps (unused), clustered decals (unused), the bindless
`StandardMaterial` path (materials then get one bind group each instead of sharing slabs), and the
array variants of the prepass, deferred and SSR shaders. The bindless material path is the only
loss that could matter, as CPU work on the render thread when there are many distinct materials.
Bevy has no narrower switch: probes, lightmaps, decals and bindless materials all test the same
features and limits, and lowering the binding-array limits would break bindless materials instead.

CPU times in the sandbox (llvmpipe, release build, Low, 8 s each; a software rasteriser, so
`render_ms` is dominated by rasterising and says nothing about a real GPU):

| scene | mode | avg ms | main ms | render ms |
| --- | --- | --- | --- | --- |
| 200 boxes | array | 304.6 | 4.6 | 303.8 |
| 200 boxes | single | 285.5 | 12.0 | 284.5 |
| lookdev | array | 354.9 | 7.2 | 353.8 |
| lookdev | single | 323.7 | 7.3 | 322.8 |
| sandbox rover | array | 374.7 | 8.2 | 373.8 |
| sandbox rover | single | 344.0 | 6.8 | 342.9 |

Main-thread time is the same within run-to-run noise (the 200-box main figure varies a lot between
runs). The render-thread figure includes the software GPU, so it cannot show a CPU cost of losing
bindless. Owner follow-up: on the laptop, compare `cpu main_ms` and `render_ms` of the 200-box
scene for `--env-mode array` and `single` (GPU-bound there, so a CPU difference would show in
`render_ms`).
