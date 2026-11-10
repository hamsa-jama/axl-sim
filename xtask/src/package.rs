//! `cargo xtask package`: build the release binary and pack it for this OS.
//!
//! Linux: `AXL-<version>-linux.tar.gz` with `libmujoco` next to `axl` (rpath `$ORIGIN`).
//! Windows: `AXL-<version>-windows.zip` with `axl.exe` and `mujoco.dll`.
//! macOS: `AXL-<version>-macos.dmg` holding an unsigned `AXL.app`; the assets, SDK and templates
//! sit next to the binary inside `Contents/MacOS`, which is where `axl` looks for them.
//! Reference solutions (`missions/solutions/`) are never packed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::fetch_mujoco::host_os;
use crate::mujoco_env::mujoco_env;

/// Files and folders that every package carries next to the binary.
const SHIPPED_DIRS: [&str; 3] = ["assets", "sdk", "templates"];

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from).with_context(|| from.display().to_string())? {
        let e = e?;
        let name = e.file_name();
        // Build output and backups never ship.
        if name == "target" || name.to_string_lossy().ends_with(".bak") {
            continue;
        }
        let dest = to.join(&name);
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

fn version(root: &Path) -> Result<String> {
    let text = fs::read_to_string(root.join("Cargo.toml"))?;
    let mut in_pkg = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[workspace.package]";
        } else if in_pkg && let Some(v) = t.strip_prefix("version") {
            return Ok(v
                .trim_start_matches([' ', '='])
                .trim()
                .trim_matches('"')
                .to_owned());
        }
    }
    bail!("no [workspace.package] version in Cargo.toml")
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let status = cmd.status().with_context(|| format!("starting {what}"))?;
    if !status.success() {
        bail!("{what} failed with {status}");
    }
    Ok(())
}

/// Build and pack.
pub fn run_package(root: &Path) -> Result<()> {
    let os = host_os()?;
    let ver = version(root)?;
    let env = mujoco_env(root)?;

    // The binary finds its MuJoCo library next to itself.
    let rpath = match os {
        "linux" => "-C link-arg=-Wl,-rpath,$ORIGIN",
        "macos" => "-C link-arg=-Wl,-rpath,@executable_path",
        _ => "",
    };
    let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    build
        .current_dir(root)
        .args(["build", "--release", "--package", "axl-app"]);
    for (k, v) in &env {
        build.env(k, v);
    }
    if !rpath.is_empty() {
        build.env("RUSTFLAGS", rpath);
    }
    run(&mut build, "cargo build --release")?;

    let out = root.join("target/package");
    let name = format!("AXL-{ver}-{os}");
    let stage = out.join(&name);
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    // Where the binary and its neighbours go.
    let bin_dir = if os == "macos" {
        stage.join("AXL.app/Contents/MacOS")
    } else {
        stage.clone()
    };
    fs::create_dir_all(&bin_dir)?;
    let exe = if os == "windows" { "axl.exe" } else { "axl" };
    fs::copy(root.join("target/release").join(exe), bin_dir.join(exe))?;

    // MuJoCo.
    let lib_dir = env
        .iter()
        .find(|(k, _)| k == "MUJOCO_DYNAMIC_LINK_DIR")
        .map(|(_, v)| PathBuf::from(v))
        .context("MUJOCO_DYNAMIC_LINK_DIR is not set")?;
    let (src_dir, prefix, suffixes): (PathBuf, &str, &[&str]) = match os {
        "windows" => (
            lib_dir.parent().unwrap_or(&lib_dir).join("bin"),
            "mujoco",
            &[".dll"],
        ),
        "macos" => (lib_dir, "libmujoco", &[".dylib"]),
        _ => (lib_dir, "libmujoco", &[".so", ".so.3.12.0"]),
    };
    let mut copied = 0;
    for e in fs::read_dir(&src_dir).with_context(|| src_dir.display().to_string())? {
        let e = e?;
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(prefix) && suffixes.iter().any(|s| n.ends_with(s)) {
            fs::copy(e.path(), bin_dir.join(&n))?;
            copied += 1;
        }
    }
    if copied == 0 {
        bail!("no MuJoCo library found in {}", src_dir.display());
    }

    for d in SHIPPED_DIRS {
        copy_dir(&root.join(d), &bin_dir.join(d))?;
    }
    let docs = stage.join("docs");
    fs::create_dir_all(&docs)?;
    fs::copy(root.join("docs/guide.md"), docs.join("guide.md"))?;
    fs::copy(root.join("README.md"), stage.join("README.md"))?;
    for licence in ["LICENSE-MIT", "LICENSE-APACHE", "LICENSE"] {
        if root.join(licence).is_file() {
            fs::copy(root.join(licence), stage.join(licence))?;
        }
    }
    // The accuracy report that About shows.
    let report = root.join("target/validation/report.md");
    if report.is_file() {
        fs::copy(&report, bin_dir.join("accuracy-report.md"))?;
        fs::copy(&report, stage.join("accuracy-report.md"))?;
    } else {
        println!("note: target/validation/report.md is missing; run `cargo xtask validate` first");
    }

    match os {
        "linux" => run(
            Command::new("tar")
                .current_dir(&out)
                .args(["-czf", &format!("{name}.tar.gz"), &name]),
            "tar",
        )?,
        "windows" => run(
            // bsdtar, which ships with Windows 10 and later, writes a zip with -a.
            Command::new("tar").current_dir(&out).args([
                "-a",
                "-cf",
                &format!("{name}.zip"),
                &name,
            ]),
            "tar",
        )?,
        _ => {
            fs::write(
                stage.join("AXL.app/Contents/Info.plist"),
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\"><dict>\n\
<key>CFBundleName</key><string>AXL</string>\n\
<key>CFBundleIdentifier</key><string>org.axl.AXL</string>\n\
<key>CFBundleExecutable</key><string>axl</string>\n\
<key>CFBundleVersion</key><string>{ver}</string>\n\
<key>CFBundleShortVersionString</key><string>{ver}</string>\n\
<key>CFBundlePackageType</key><string>APPL</string>\n\
<key>NSHighResolutionCapable</key><true/>\n\
</dict></plist>\n"
                ),
            )?;
            run(
                Command::new("hdiutil")
                    .current_dir(&out)
                    .args(["create", "-volname", "AXL", "-srcfolder", &name, "-ov"])
                    .arg(format!("{name}.dmg")),
                "hdiutil",
            )?;
        }
    }
    println!("package written to {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_comes_from_the_workspace_package() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let v = version(&root).unwrap();
        assert!(v.split('.').count() == 3, "{v}");
    }

    #[test]
    fn copying_leaves_out_target_folders_and_backups() {
        let base = std::env::temp_dir().join("axl-package-test");
        let _ = fs::remove_dir_all(&base);
        let (src, dst) = (base.join("src"), base.join("dst"));
        fs::create_dir_all(src.join("target")).unwrap();
        fs::create_dir_all(src.join("keep")).unwrap();
        fs::write(src.join("target/junk"), "x").unwrap();
        fs::write(src.join("keep/a.txt"), "a").unwrap();
        fs::write(src.join("robot.axlbot.bak"), "b").unwrap();
        copy_dir(&src, &dst).unwrap();
        assert!(dst.join("keep/a.txt").is_file());
        assert!(!dst.join("target").exists());
        assert!(!dst.join("robot.axlbot.bak").exists());
        let _ = fs::remove_dir_all(&base);
    }
}
