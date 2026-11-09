# 0005: WESL for the grid shader, and a procedural studio environment

Status: accepted (M2). Two small departures from the spec text, both forced by the platform.

## 1. The grid shader is WESL, not plain WGSL

### Problem

The spec says GPU code must be WGSL. Bevy 0.20 replaced its shader preprocessor (`naga_oil`) with
WESL, and every Bevy shader module (`bevy_pbr::render::pbr_functions`, `pbr_fragment`, ...) is now
WESL. A plain `.wgsl` file cannot `import` them. The spec also asks for grid lines that darken the
floor colour *before lighting*, so they receive shadows, contact shadows and ambient occlusion like
printed ink. That needs Bevy's PBR input and lighting functions.

### Options

1. Write plain WGSL with no imports and re-implement lighting, shadows and SSAO. Far too large, and
   it would drift from Bevy's lighting.
2. Write the grid shader in WESL, which is a strict superset of WGSL (every WGSL file is valid
   WESL). Keep the code WGSL-style and add only the `import` lines Bevy requires.

### Decision

Option 2. `crates/axl-render/src/shaders/grid_floor.wesl` is ordinary WGSL plus `import` lines. It is
the only shader in AXL so far. The formulas it uses are mirrored by tested Rust functions in
`grid_math.rs`.

## 2. The studio environment is generated in code

### Problem

The spec asks for a neutral indoor studio HDRI from Poly Haven (CC0, 2K). The development sandbox
blocks `dl.polyhaven.org` (HTTP 403 from the egress proxy), so the file cannot be fetched here.

### Options

1. Commit a placeholder file and ask the owner to supply the HDRI.
2. Generate a studio cubemap procedurally at start-up (grey room, two softboxes, a ceiling strip
   light, a lighter floor bounce) and let Bevy filter it on the GPU with
   `GeneratedEnvironmentMapLight`. Bevy 0.20 can filter environment maps at runtime, so no
   `cargo xtask envmap` is needed (the spec allows this).

### Decision

Option 2. `lighting::studio_cubemap` builds an `Rgba16Float` cubemap from `studio_radiance`, which is
our own function, so there are no third-party assets and no licence question. The key softbox sits at
the same azimuth as the key light so highlights and shadows agree.

### Owner action (optional)

If the Poly Haven HDRI is wanted for the final look, it can replace `studio_cubemap`: convert it to
a cubemap image and hand it to `GeneratedEnvironmentMapLight`. Nothing else depends on the
procedural version. The look-dev screenshots in `docs/screenshots/` were made with the procedural
studio.
