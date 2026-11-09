//! The `axl` binary. See `README.md` in this crate.

mod cli;
#[cfg(feature = "gui")]
mod gui;
mod headless;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Args;

fn main() -> ExitCode {
    let args = Args::parse();
    init_logging(args.headless);

    if args.headless {
        return headless::run(&args);
    }
    run_gui(&args)
}

#[cfg(feature = "gui")]
fn run_gui(args: &Args) -> ExitCode {
    if let Some(problem) = display_problem() {
        eprintln!("axl: {problem}");
        return ExitCode::FAILURE;
    }
    match gui::run(args) {
        bevy::app::AppExit::Success => ExitCode::SUCCESS,
        bevy::app::AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

#[cfg(not(feature = "gui"))]
fn run_gui(_args: &Args) -> ExitCode {
    eprintln!("axl: this build has no window. Use --headless, or rebuild with the `gui` feature.");
    ExitCode::from(2)
}

/// Headless runs log to stderr only; the GUI installs Bevy's own logger.
fn init_logging(headless: bool) {
    if headless || cfg!(not(feature = "gui")) {
        // Ignore the error: a logger may already be installed (tests).
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }
}

/// A message if the desktop session obviously has no display to open a window on.
#[cfg(all(feature = "gui", target_os = "linux"))]
fn display_problem() -> Option<&'static str> {
    let has = |var| std::env::var_os(var).is_some_and(|v| !v.is_empty());
    if has("DISPLAY") || has("WAYLAND_DISPLAY") {
        None
    } else {
        Some(
            "no display found (DISPLAY and WAYLAND_DISPLAY are unset). Use --headless on a server.",
        )
    }
}

/// Windows and macOS always have a desktop session.
#[cfg(all(feature = "gui", not(target_os = "linux")))]
fn display_problem() -> Option<&'static str> {
    None
}
