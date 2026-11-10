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

    /// Save a screenshot of the window to this path once the image has settled, then exit.
    #[arg(long, value_name = "PATH", conflicts_with = "headless")]
    pub screenshot: Option<PathBuf>,

    /// Render this many boxes moving through the snapshot path, for the frame-rate check.
    #[arg(long, value_name = "N", conflicts_with_all = ["headless", "lookdev"])]
    pub stress_boxes: Option<usize>,

    /// Drive a starter robot (`rover` or `test_chassis`) on the sim thread with fixed voltages.
    #[arg(long, value_name = "STARTER", conflicts_with_all = ["headless", "lookdev", "stress_boxes", "parts", "builder"])]
    pub sandbox: Option<String>,

    /// Open the Code screen for this mission (for example `m01`): editor, API reference, build
    /// log, and a Rover that runs the built program.
    #[arg(long, value_name = "MISSION", conflicts_with_all = ["headless", "lookdev", "stress_boxes", "parts", "builder", "sandbox"])]
    pub code: Option<String>,

    /// With --code: start with this language, `c` or `rust`.
    #[arg(long, value_name = "LANG", requires = "code")]
    pub code_lang: Option<String>,

    /// With --code: build and run the program as soon as the screen opens.
    #[arg(long, requires = "code")]
    pub code_run: bool,

    /// With --sandbox: motor voltages, for example `left=4,right=4`.
    #[arg(long, value_name = "LIST", requires = "sandbox")]
    pub volts: Option<String>,

    /// Open the Builder screen.
    #[arg(long, conflicts_with_all = ["headless", "lookdev", "stress_boxes", "parts"])]
    pub builder: bool,

    /// Open the part browser. Takes a filter (id, name, category or `all`).
    #[arg(long, value_name = "FILTER", conflicts_with_all = ["headless", "lookdev", "stress_boxes"])]
    pub parts: Option<String>,

    /// With --parts: isolate this one part id.
    #[arg(long, value_name = "ID", requires = "parts")]
    pub part: Option<String>,

    /// With --parts: colour id for every part (for example red).
    #[arg(long, value_name = "COLOR", requires = "parts", hide = true)]
    pub part_color: Option<String>,

    /// With --part: draw connectors.
    #[arg(long, requires = "parts")]
    pub connectors: bool,

    /// With --part: draw collision shapes.
    #[arg(long, requires = "parts")]
    pub collision: bool,

    /// Exit as soon as the first-launch benchmark has finished (to check what it picks).
    #[arg(long, hide = true, conflicts_with = "headless")]
    pub exit_after_benchmark: bool,

    /// Switch effects off to find what a GPU or driver cannot handle. Comma-separated list of:
    /// grid, env, ssao, taa, contact-shadows, shadows, bloom, tonemap, micro-normal, picking.
    #[arg(long, value_name = "LIST", conflicts_with = "headless")]
    pub disable: Option<String>,

    /// Which environment-light path to run. One of: single (default; masks
    /// TEXTURE_BINDING_ARRAY, see decision 0011), array (the adapter's own choice), and the
    /// diagnostics diffuse-only, specular-only, ambient (same as --disable env), no-clearcoat.
    #[arg(long, value_name = "MODE", conflicts_with = "headless")]
    pub env_mode: Option<String>,

    /// Switch every optional effect off (same as `--disable all`).
    #[arg(long, conflicts_with = "headless")]
    pub safe_render: bool,

    /// Quality preset: low, medium, high or ultra. Overrides the saved setting and skips the benchmark.
    #[arg(long, value_name = "PRESET", conflicts_with = "headless")]
    pub quality: Option<String>,

    /// Run for this many seconds, then print a frame-time summary and exit (frame-rate checks).
    #[arg(long, value_name = "SECONDS", conflicts_with = "headless")]
    pub perf_seconds: Option<f32>,

    /// Window size as WIDTHxHEIGHT, for example 960x540 (faster look-dev iteration under software rendering).
    #[arg(long, value_name = "WxH", hide = true, conflicts_with = "headless")]
    pub window_size: Option<String>,

    /// Start with the stud or metric grid: studs or metric.
    #[arg(long, value_name = "UNITS", hide = true, conflicts_with = "headless")]
    pub units: Option<String>,

    /// Start in the builder or simulate view (changes the default grid opacity).
    #[arg(long, value_name = "MODE", hide = true, conflicts_with = "headless")]
    pub view: Option<String>,

    /// Start in this camera mode: orbit, follow or top-down.
    #[arg(long, value_name = "MODE", hide = true, conflicts_with = "headless")]
    pub camera: Option<String>,

    /// Camera pose as FOCUS_X,FOCUS_Y,FOCUS_Z,YAW,PITCH,DISTANCE (millimetres and degrees).
    #[arg(long, value_name = "POSE", hide = true, conflicts_with = "headless")]
    pub camera_pose: Option<String>,

    /// Exit after this many rendered frames. For smoke tests.
    #[arg(long, value_name = "N", hide = true, conflicts_with = "headless")]
    pub exit_after_frames: Option<u32>,
}
