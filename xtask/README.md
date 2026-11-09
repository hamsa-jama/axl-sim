# xtask

Build tooling, run as `cargo xtask <command>`.

| Command | State |
| --- | --- |
| `fetch-mujoco` | Done (M0). Downloads and verifies MuJoCo 3.12.0 for this OS. |
| `validate` | Arrives with M1 (accuracy suite). |
| `sdk-check` | Arrives with M7 and M9. |
| `package` | Arrives with M10. |
| `envmap` | Only if M2 needs it. |

`fetch-mujoco` shells out to the OS tools `curl`, `tar` and (macOS) `hdiutil`, so xtask needs no HTTP or archive crates. See `docs/decisions/0001-xtask-uses-system-curl-and-tar.md`.
