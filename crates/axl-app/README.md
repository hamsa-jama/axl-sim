# axl-app

The `axl` binary: screens, wiring, and the headless command-line mode.

- `axl` opens the desktop app (needs a display and a GPU).
- `axl --headless --version` prints the version and exits; it never touches the renderer.
- Build with `--no-default-features` to leave out Bevy and egui entirely.

Only this crate and `axl-render` may depend on Bevy.
