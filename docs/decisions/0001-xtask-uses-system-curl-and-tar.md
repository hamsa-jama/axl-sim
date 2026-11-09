# 0001: xtask uses the system curl and tar

Status: accepted (M0). Spec rule 8 (simplest option that passes the checks).

## Problem

`cargo xtask fetch-mujoco` must download an archive and unpack it on Windows, macOS and Linux. The approved crate list has no HTTP client and no archive reader (`tar`, `flate2`, `zip`). Adding them needs a decision record first.

## Options

1. Add `ureq`, `flate2`, `tar` and `zip` to `xtask`.
2. Call the OS tools `curl` and `tar` (Windows 10+ ships both; macOS ships both plus `hdiutil` for the `.dmg`).

## Decision

Option 2. It adds no crates, and the SHA-256 check (with the approved `sha2` crate) is what guards integrity, not the transport. On Windows we call `%SystemRoot%\System32\tar.exe` explicitly because a GNU tar from Git for Windows cannot read zip files.

## Consequence

If a target machine lacks `curl` or `tar`, `fetch-mujoco` fails with the name of the command. Option 1 stays available through a new decision record.
