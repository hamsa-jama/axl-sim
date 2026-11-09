# 0004: pushing the m00-done tag is rejected from the development sandbox

Status: open. Needs the owner (one command) before M1 starts.

## Problem

The spec says a milestone is finished only when it is merged into `main`, tagged `mNN-done` and pushed, and that the next milestone must not start before that.

From the cloud sandbox that built M0, pushing branches works (`main`, `milestone/m00`), but pushing the tag fails every time, for both a lightweight and an annotated tag:

```
error: RPC failed; HTTP 403 curl 22 The requested URL returned error: 403
send-pack: unexpected disconnect while reading sideband packet
```

The sandbox's egress proxy README says 403 responses are policy denials and must not be retried or routed around. `gh` has an invalid `GH_TOKEN`, so the GitHub API route is not available either, and was not tried.

## Options

1. Owner runs, from any clone with push rights:
   `git fetch origin && git tag -a m00-done -m "M0 skeleton accepted" 80b4537 && git push origin m00-done`
   (`80b4537` is the M0 merge commit on `main`.) Then the engineer starts M1.
2. Owner allows tag pushes in the sandbox policy, and the engineer pushes the tag itself.

## Decision

Stop after M0. M0 is merged into `main` and pushed; only the tag is missing. M1 was not started.
