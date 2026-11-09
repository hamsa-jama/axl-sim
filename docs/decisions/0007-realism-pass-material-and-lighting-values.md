# 0007: Realism pass, values that differ from the spec text

Date: 2026-11-09. Status: accepted (M3).

## Context

The M3 owner brief asks for a product-photo look. The first M3 look-dev screenshots
(`docs/screenshots/m3-*-before-realism-1280x720.png`) were flat and matte. Work was done
by screenshot comparison under Mesa llvmpipe; a real GPU may show the result differently.

## What was found

- **Bloom washed out the whole frame.** The bloom prefilter (threshold 1.0, softness 0.6) started
  at 0.4, below the floor's brightness, so every surface glowed. Now threshold 1.3, softness 0.2.
- **Ambient light was too even.** A bright floor bounce and ceiling made diffuse light flat, and
  AgX turned the brightly lit tops of red parts pink. The studio now has a dim room, a bright
  tent (a dome that rises from 18 to 46 degrees elevation, a ring panel at 27 to 39 degrees,
  eight vertical strip lights), and the key light is 5500 lux (was 11000).
- **Glossy reflection is subtle by physics.** Plastic reflects about 4 to 6% at these angles, so a
  reflection only reads when the panels are small and much brighter than the diffuse light.

## Changes from the spec's numbers

| Value | Spec | Now | Why |
| --- | --- | --- | --- |
| Clearcoat strength | 0.15 | 0.25 | A second, sharper reflection lobe is what lets softbox shapes survive the base roughness. Higher values (0.6) made SSAO noise show as blotches on side faces. |
| Clearcoat roughness | 0.2 | 0.15 | Same reason. |
| Micro-surface normal strength | 0.15 | 0.025, finer grain (3 octaves at 0.5, 0.25 and 0.125 mm) | At 0.15, and with the original 2 mm blotches, the grain read as stains. |
| Floor base colour | not specified | 0.66 grey (was 0.78) | Parts need contrast against the floor. |
| Bloom | on | threshold raised (see above) | Haze. |

Everything else in the spec's material table is unchanged (roughness 0.28, reflectance 0.5, rubber and steel).
The studio is still procedural; a Poly Haven HDRI dropped at `assets/render/studio.hdr` replaces it
(see the owner checks).

## Known problem

SSAO (High preset, medium quality) shows faint blotches and a pale halo on the sides of glossy parts under
llvmpipe in a single screenshot. They disappear with `--disable ssao`. The cause is SSAO noise being
multiplied into the stronger specular term; it is likely smaller on a real GPU where TAA has accumulated
many frames, but that has not been checked. Reflections are clearer than before but still softer than a
true area-light render; the owner should judge on a real GPU (check O7).
