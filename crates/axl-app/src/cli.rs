//! Command-line arguments for the `axl` binary.

use std::path::PathBuf;

use clap::Parser;

/// AXL: a robotics simulator for teenagers.
#[derive(Debug, Parser)]
#[command(name = "axl", version, about, long_about = None)]
pub struct Args {
    /// Run without a window or a GPU. Used by graders and CI.
    #[arg(long)]
    pub headless: bool,

    /// Mission id to run in headless mode, for example `m03`.
    #[arg(long, value_name = "ID", requires = "headless")]
    pub mission: Option<String>,

    /// Compiled controller (`.wasm`) to run in headless mode.
    #[arg(long, value_name = "PATH", requires = "headless")]
    pub controller: Option<PathBuf>,

    /// Seed for every source of randomness in a run.
    #[arg(long, value_name = "N", default_value_t = 1)]
    pub seed: u64,

    /// Render the look-dev scene (milestone M2).
    #[arg(long, conflicts_with = "headless")]
    pub lookdev: bool,

    /// With `--lookdev`, save a screenshot to this path and exit.
    #[arg(long, value_name = "PATH", requires = "lookdev")]
    pub screenshot: Option<PathBuf>,

    /// Exit after this many rendered frames. For smoke tests.
    #[arg(long, value_name = "N", hide = true, conflicts_with = "headless")]
    pub exit_after_frames: Option<u32>,
}
