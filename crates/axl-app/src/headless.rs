//! Headless mode: no window, no GPU.

use std::process::ExitCode;

use crate::cli::Args;

/// Exit code for a run that was asked to do something not built yet.
const EXIT_UNSUPPORTED: u8 = 2;

/// Run in headless mode and return the process exit code.
///
/// Today this only supports `--version` (handled by clap before we get here),
/// so a bare `--headless` prints the version line. Mission grading arrives in
/// milestone M9.
pub fn run(args: &Args) -> ExitCode {
    if args.mission.is_some() || args.controller.is_some() {
        eprintln!("axl: headless mission grading is not available yet (milestone M9).");
        return ExitCode::from(EXIT_UNSUPPORTED);
    }
    println!("{}", version_line());
    ExitCode::SUCCESS
}

/// The one-line version string, `axl <version>`.
pub fn version_line() -> String {
    format!("axl {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_has_package_version() {
        assert_eq!(version_line(), format!("axl {}", env!("CARGO_PKG_VERSION")));
    }
}
