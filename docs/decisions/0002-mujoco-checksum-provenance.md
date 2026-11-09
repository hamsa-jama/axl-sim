# 0002: provenance of the pinned MuJoCo checksums

Status: accepted (M0). Owner action suggested.

## Problem

The spec says to copy the SHA-256 values from the official release page. The sandbox that built M0 cannot reach `api.github.com`, and release pages do not show digests in the HTML it can fetch.

## Decision

The three archives were downloaded over HTTPS from
`https://github.com/google-deepmind/mujoco/releases/download/3.12.0/` and hashed with `sha256sum`. The hashes are in `xtask/src/mujoco_checksums.rs`. The Linux archive was also unpacked and its `libmujoco.so.3.12.0` is present.

## Owner action (optional)

Compare the three values with the digests GitHub shows under "Assets" on
`https://github.com/google-deepmind/mujoco/releases/tag/3.12.0`. If one differs, change the table; nothing else depends on it.
