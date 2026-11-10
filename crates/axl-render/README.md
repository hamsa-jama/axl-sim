# axl-render

Bevy rendering for AXL: materials, mesh upload, grid floor, overlays, camera, quality presets and the Y-up conversion.

Part of the AXL workspace. See `SPEC.md` for the design and `docs/architecture.md` for how the crates fit together.

M8: `instruments` draws the seven overlays (frames, balance, contacts, sensors, trails, drawings,
X-ray) as gizmos from `LiveView` and `Trails`, which the app fills; keys 1 to 7 toggle them.
