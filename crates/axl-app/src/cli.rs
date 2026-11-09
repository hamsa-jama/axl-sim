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

    /// Render this many boxes moving through the snapshot path, for the frame-rate check.
    #[arg(long, value_name = "N", conflicts_with_all = ["headless", "lookdev"])]
    pub stress_boxes: Option<usize>,

    /// Quality preset: low, medium, high or ultra. Overrides the saved setting and skips the benchmark.
    #[arg(long, value_name = "PRESET", conflicts_with = "headless")]
    pub quality: Option<String>,

    /// Run for this many seconds, then print a frame-time summary and exit (frame-rate checks).
    #[arg(long, value_name = "SECONDS", conflicts_with = "headless")]
    pub perf_seconds: Option<f32>,

    /// Exit after this many rendered frames. For smoke tests.
    #[arg(long, value_name = "N", hide = true, conflicts_with = "headless")]
    pub exit_after_frames: Option<u32>,
}
