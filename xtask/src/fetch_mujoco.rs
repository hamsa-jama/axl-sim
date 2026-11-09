//! `cargo xtask fetch-mujoco`: download, verify and unpack MuJoCo.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::mujoco_checksums::{Archive, ArchiveKind, MUJOCO_VERSION, archive_for};

/// Name of the file that marks a finished, verified extraction.
const MARKER: &str = ".axl-verified";

/// The OS this build of xtask runs on, as a key of the checksum table.
pub fn host_os() -> Result<&'static str> {
    if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Ok("linux")
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        Ok("windows")
    } else if cfg!(target_os = "macos") {
        Ok("macos")
    } else {
        bail!("no MuJoCo {MUJOCO_VERSION} archive is pinned for this OS and CPU")
    }
}

/// Download URL of a release asset.
pub fn download_url(file: &str) -> String {
    format!("https://github.com/google-deepmind/mujoco/releases/download/{MUJOCO_VERSION}/{file}")
}

/// Lower-case hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Run the command and fail with its name if it does not succeed.
fn run(cmd: &mut Command) -> Result<()> {
    let name = format!("{cmd:?}");
    let status = cmd
        .status()
        .with_context(|| format!("could not start {name}"))?;
    if !status.success() {
        bail!("{name} failed with {status}");
    }
    Ok(())
}

/// Where the library ends up for an OS.
pub fn install_dir(root: &Path, os: &str) -> PathBuf {
    root.join("third_party")
        .join("mujoco")
        .join(MUJOCO_VERSION)
        .join(os)
}

/// Download and unpack MuJoCo for this OS under `root/third_party`.
pub fn run_fetch(root: &Path) -> Result<()> {
    let os = host_os()?;
    let archive = archive_for(os).context("no archive for this OS")?;
    let dest = install_dir(root, os);

    if fs::read_to_string(dest.join(MARKER)).is_ok_and(|m| m.trim() == archive.sha256) {
        println!(
            "MuJoCo {MUJOCO_VERSION} is already installed in {}",
            dest.display()
        );
        print_env(&dest, os);
        return Ok(());
    }

    let downloads = root.join("third_party").join("downloads");
    fs::create_dir_all(&downloads)?;
    let file = downloads.join(archive.file);
    ensure_downloaded(archive, &file)?;

    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(&dest)?;
    extract(archive, &file, &dest)?;
    fs::write(dest.join(MARKER), format!("{}\n", archive.sha256))?;
    println!("Installed MuJoCo {MUJOCO_VERSION} in {}", dest.display());
    print_env(&dest, os);
    Ok(())
}

fn ensure_downloaded(archive: &Archive, file: &Path) -> Result<()> {
    if file.exists() && sha256_file(file)? == archive.sha256 {
        println!("Using cached {}", file.display());
        return Ok(());
    }
    let url = download_url(archive.file);
    println!("Downloading {url}");
    run(Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--retry",
            "3",
            "--output",
        ])
        .arg(file)
        .arg(&url))?;
    let got = sha256_file(file)?;
    if got != archive.sha256 {
        // Never keep an archive that failed its check.
        let _ = fs::remove_file(file);
        bail!(
            "checksum mismatch for {}: expected {}, got {got}",
            archive.file,
            archive.sha256
        );
    }
    Ok(())
}

fn extract(archive: &Archive, file: &Path, dest: &Path) -> Result<()> {
    match archive.kind {
        ArchiveKind::TarGz => run(Command::new("tar")
            .arg("-xzf")
            .arg(file)
            .arg("-C")
            .arg(dest)
            .args(["--strip-components", "1"])),
        // Windows 10+ ships bsdtar as `tar`, which reads zip files.
        ArchiveKind::Zip => run(Command::new(zip_tar())
            .arg("-xf")
            .arg(file)
            .arg("-C")
            .arg(dest)),
        ArchiveKind::Dmg => extract_dmg(file, dest),
    }
}

/// The `tar` that can read zip files. On Windows that is the system bsdtar, not
/// a GNU tar that Git may have put earlier on `PATH`.
fn zip_tar() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("tar.exe")
    } else {
        PathBuf::from("tar")
    }
}

fn extract_dmg(file: &Path, dest: &Path) -> Result<()> {
    let mount = dest.with_extension("mnt");
    fs::create_dir_all(&mount)?;
    run(Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(file))?;
    let copied =
        find_framework(&mount, 3).map(|fw| run(Command::new("cp").arg("-R").arg(&fw).arg(dest)));
    // Always detach, even if the copy failed.
    let detached = run(Command::new("hdiutil").arg("detach").arg(&mount));
    let _ = fs::remove_dir(&mount);
    match copied {
        Some(r) => r?,
        None => bail!("mujoco.framework not found inside {}", file.display()),
    }
    detached
}

fn find_framework(dir: &Path, depth: u8) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == "mujoco.framework") {
            return Some(path);
        }
        if depth > 0
            && path.is_dir()
            && let Some(found) = find_framework(&path, depth - 1)
        {
            return Some(found);
        }
    }
    None
}

fn print_env(dest: &Path, os: &str) {
    println!();
    println!("To build AXL against this copy, see docs/build.md. Library location:");
    match os {
        "linux" => println!("  {}", dest.join("lib").display()),
        "windows" => println!("  {}", dest.join("bin").display()),
        _ => println!("  {}", dest.join("mujoco.framework").display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mujoco_checksums::ARCHIVES;

    #[test]
    fn urls_point_at_the_pinned_release() {
        for a in ARCHIVES {
            let url = download_url(a.file);
            assert!(url.contains("/releases/download/3.12.0/"), "{url}");
            assert!(url.ends_with(a.file));
        }
    }

    #[test]
    fn checksums_are_64_lowercase_hex_chars() {
        for a in ARCHIVES {
            assert_eq!(a.sha256.len(), 64, "{}", a.file);
            assert!(
                a.sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
        }
    }

    #[test]
    fn sha256_matches_known_vector() {
        let dir = std::env::temp_dir().join(format!("axl-xtask-sha-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc.txt");
        fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn install_dir_layout() {
        let d = install_dir(Path::new("/r"), "linux");
        assert!(d.ends_with("third_party/mujoco/3.12.0/linux"));
    }

    #[test]
    fn every_os_has_an_archive() {
        for os in ["linux", "windows", "macos"] {
            assert!(archive_for(os).is_some());
        }
        assert!(archive_for("plan9").is_none());
    }
}
