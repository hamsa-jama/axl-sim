# Calibration list

Every placeholder physical value in AXL is listed here with how to measure it on real hardware. Calibration is owner work and is not part of version 1 acceptance.

The list is filled in as `assets/physics/*.ron` appears (milestones M3 and M6).

| Value | File | Default | How to measure |
| --- | --- | --- | --- |
| Quality benchmark cut-offs (`ultra_max_ms`, `high_max_ms`, `medium_max_ms`) | `assets/render/quality.ron` | 6, 12 and 20 ms average frame time over 3 s at High | Not a physical value. Run `axl --stress-boxes 200 --quality high --perf-seconds 20` on reference machines (see `docs/perf.md`) and set the cut-offs so each class of GPU lands on the preset that meets its target. |
