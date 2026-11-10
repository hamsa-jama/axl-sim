//! `cargo xtask`: build tooling for AXL.

mod fetch_mujoco;
mod mujoco_checksums;
mod mujoco_env;
mod sdk_check;

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
    /// Print the variables needed to build against the fetched MuJoCo, as NAME=value lines.
    MujocoEnv,
    /// Run the accuracy suite and write target/validation/report.md and report.json.
    Validate,
    /// Build every template and reference solution and run them (M9).
    SdkCheck {
        /// Fail if wasi-sdk is missing instead of skipping the C projects.
        #[arg(long)]
        require_c: bool,
        /// Fail if the wasm32-wasip1 target is missing instead of only running cargo check.
        #[arg(long)]
        require_rust: bool,
    },
    /// Build the release package for this OS (arrives with M10).
    Package,
    /// Write assets/parts/*.ron from the generator in axl-parts.
    GenParts,
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
        Command::MujocoEnv => {
            for (k, v) in mujoco_env::mujoco_env(&workspace_root())? {
                println!("{k}={v}");
            }
            Ok(())
        }
        Command::Validate => validate(),
        Command::SdkCheck {
            require_c,
            require_rust,
        } => sdk_check::run(
            &workspace_root(),
            sdk_check::Options {
                require_c,
                require_rust,
            },
        ),
        Command::Package => not_yet("package", "M10"),
        Command::GenParts => {
            let n = axl_parts::catalog::write_builtin(&workspace_root().join("assets/parts"))?;
            println!("wrote {n} part files to assets/parts");
            Ok(())
        }
        Command::Envmap => not_yet("envmap", "M2"),
    }
}

fn not_yet(name: &str, milestone: &str) -> Result<()> {
    bail!("`cargo xtask {name}` is not implemented yet; it arrives with milestone {milestone}")
}

/// Build and run `axl-validate` in release mode with MuJoCo on the library path.
fn validate() -> Result<()> {
    let root = workspace_root();
    let mut cmd =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(&root)
        .args(["run", "--release", "--package", "axl-validate", "--"]);
    for (k, v) in mujoco_env::mujoco_env(&root)? {
        cmd.env(k, v);
    }
    let status = cmd.status()?;
    if !status.success() {
        bail!("the accuracy suite failed (see the report in target/validation/)");
    }
    Ok(())
}
