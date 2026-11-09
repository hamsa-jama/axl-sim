//! `cargo xtask`: build tooling for AXL.

mod fetch_mujoco;
mod mujoco_checksums;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "Build tooling for AXL")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download and verify MuJoCo 3.12.0 for this OS.
    FetchMujoco,
    /// Run the accuracy suite and write the report (arrives with M1).
    Validate,
    /// Build every template and reference solution and run them (arrives with M7).
    SdkCheck,
    /// Build the release package for this OS (arrives with M10).
    Package,
    /// Prefilter the HDRI into KTX2 cubemaps, only if M2 needs it.
    Envmap,
}

/// The workspace root: the parent of this crate's folder.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::FetchMujoco => fetch_mujoco::run_fetch(&workspace_root()),
        Command::Validate => not_yet("validate", "M1"),
        Command::SdkCheck => not_yet("sdk-check", "M7"),
        Command::Package => not_yet("package", "M10"),
        Command::Envmap => not_yet("envmap", "M2"),
    }
}

fn not_yet(name: &str, milestone: &str) -> Result<()> {
    bail!("`cargo xtask {name}` is not implemented yet; it arrives with milestone {milestone}")
}
