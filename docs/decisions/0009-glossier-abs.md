# 0009: Glossier ABS (supersedes the spec's ABS material row)

Date: 2026-11-09. Status: accepted (owner authorised the ranges; spec author).

## Context

Decision 0008 found that at the spec's ABS perceptual roughness 0.28, Bevy's prefiltered
environment mips blur the small softboxes away, so the red brick read as matte painted clay. The
spec author authorised: roughness 0.08 to 0.20, clearcoat 0 to 0.5 with clearcoat roughness 0.03
to 0.15, reflectance 0.4 to 0.6.

## What was tried

Close-up of the red 2x4 brick at High under Mesa llvmpipe (1280x720, camera 59,-43,8 mm, yaw -35,
pitch 32, distance 80 mm), plus pitch 50 to catch the overhead softboxes on the top face.

| Roughness | Clearcoat / roughness | Result |
| --- | --- | --- |
| 0.28 (before) | 0.25 / 0.15 | Matte clay. Faint stud highlights. |
| 0.18 | 0 and 0.3 / 0.06 | Soft stud highlights, sides dull. |
| 0.14 | 0 and 0.3 / 0.06 | Better, still soft. |
| 0.10 | 0 | Clear bright shapes on stud tops, colour gradient on the sides, saturated red. |
| 0.10 | 0.3 / 0.06 | Clearcoat washes a pink veil over the red; less saturated than 0.10 without it. |
| 0.08 | 0 | Slightly crisper than 0.10; the micro-surface grain starts to sparkle on the top face. |

## Chosen values

| Value | Spec | Now |
| --- | --- | --- |
| ABS perceptual roughness | 0.28 | 0.10 (per-part jitter of +-0.03 stays) |
| Clearcoat | 0.15 (0.25 in 0007) | 0.10 |
| Clearcoat roughness | 0.2 (0.15 in 0007) | 0.05 |
| Reflectance | 0.5 | 0.5 (unchanged) |
| Micro-surface normal strength | 0.15 (0.025 in 0007) | 0.012 |

The micro-surface strength was lowered because at roughness 0.10 the grain shows as orange peel
sparkle on the top face (it was invisible at 0.28). The 0008 softbox layout was left alone: at
roughness 0.10 the stud tops show the panels as clear bright shapes, and a wide sheen of the
overhead panel appears on the top face when the camera looks down steeply (pitch 50).

## SSAO

Compared High with and without `--disable ssao` at roughness 0.10. No blotches or speckle were
visible on the glossy faces in either image, so SSAO was not changed. (0007 saw blotches with the
old clearcoat 0.6; at 0.10 they do not appear under llvmpipe.) Check on a real GPU (owner check).

## Not done

No custom CPU-baked environment map (excluded by the owner). Steel (roughness 0.15) and rubber are
unchanged. Transparent ABS has no material in code yet.
