# axl-world

World and object definitions, and arena building (M9). A world is a RON file in `assets/worlds/`
(mat, walls, statics, loose objects, spawn, zones, variants); `build_world` returns the MJCF for
`<worldbody>`, the zones, the spawn and the rasterised mat (2 mm pixels, linear RGB) that the colour
sensor reads. The six shipped worlds are `open_grid`, `test_track`, `line_track` (variants `a`, `b`),
`walled_arena` (`straight`, `angled`), `maze` (three layouts chosen by the run seed) and `ramp_lab`
(`ramp_10` to `ramp_25`); the seven objects are in `assets/objects/`. No Bevy, no MuJoCo.

Part of the AXL workspace. See `SPEC.md` for the design and `docs/architecture.md` for how the crates fit together.
