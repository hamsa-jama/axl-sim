//! Environment variables that make Cargo link and run against the MuJoCo
//! copy that `fetch-mujoco` installed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::fetch_mujoco::{host_os, install_dir};

/// `(name, value)` pairs to set before building or running anything that
/// uses `axl-mujoco`.
pub fn mujoco_env(root: &Path) -> Result<Vec<(String, String)>> {
    let os = host_os()?;
    let dir = install_dir(root, os);
    if !dir.exists() {
        bail!("MuJoCo is not installed yet. Run `cargo xtask fetch-mujoco` first.");
    }
    let (link_dir, run_var, run_dir) = match os {
        "linux" => (dir.join("lib"), "LD_LIBRARY_PATH", dir.join("lib")),
        "windows" => (dir.join("lib"), "PATH", dir.join("bin")),
        _ => {
            let lib = find_dylib_dir(&dir.join("mujoco.framework"))
                .context("could not find libmujoco*.dylib inside mujoco.framework")?;
            (lib.clone(), "DYLD_LIBRARY_PATH", lib)
        }
    };
    let joined = join_path_var(run_var, &run_dir);
    Ok(vec![
        (
            "MUJOCO_DYNAMIC_LINK_DIR".to_owned(),
            link_dir.display().to_string(),
        ),
        (run_var.to_owned(), joined),
    ])
}

/// `dir` prepended to the current value of a search-path variable.
fn join_path_var(var: &str, dir: &Path) -> String {
    let mut parts = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os(var) {
        parts.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(parts)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| dir.display().to_string())
}

fn find_dylib_dir(dir: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if let Some(found) = find_dylib_dir(&path) {
                return Some(found);
            }
        } else if name.starts_with("libmujoco") && name.ends_with(".dylib") {
            return path.parent().map(Path::to_path_buf);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_var_puts_new_directory_first() {
        let joined = join_path_var("AXL_TEST_UNSET_VAR", Path::new("/x/lib"));
        assert_eq!(joined, "/x/lib");
    }

    #[test]
    fn missing_install_is_a_clear_error() {
        let err = mujoco_env(Path::new("/definitely/not/here")).unwrap_err();
        assert!(err.to_string().contains("fetch-mujoco"));
    }
}
