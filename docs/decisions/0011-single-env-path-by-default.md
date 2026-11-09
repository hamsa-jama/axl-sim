# 0011: the single-cube environment path is the default

Date: 2026-11-09. Status: accepted (rendering performance; no locked decision changed).

## Context

The owner measured the environment-light variants on the reference laptop (Intel UHD 620, Low,
`--lookdev`, `docs/perf.md`). The default (binding-array, clustered light-probe path) took 22.5 ms in
the opaque pass; forcing the single-cube path took 14.1 ms with the same lighting. Diffuse-only,
specular-only and no-clearcoat barely moved, so the cost is the array path itself. AXL has exactly
one environment map, so the array path buys nothing.

## Decision

- `EnvMode::Single` is the default for every preset: `TEXTURE_BINDING_ARRAY` is masked when the
  device is created. `--env-mode array` and the other modes remain as diagnostics.
- No narrower switch exists in Bevy 0.20.1. The probe arrays, lightmaps, clustered decals and
  bindless materials all go through `binding_arrays_are_usable` or test `TEXTURE_BINDING_ARRAY`
  directly. The loss is the bindless `StandardMaterial` path (per-material bind groups instead of
  shared slabs), which only costs CPU time with many distinct materials. The sandbox cannot measure
  that (llvmpipe); the numbers and an owner follow-up are in `docs/perf.md`.

## Consequence

If a future feature needs reflection probes, irradiance volumes, clustered decals or lightmaps, this
mask must be revisited.
