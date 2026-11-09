//! `.axlbot` files: RON with a version field, atomic writes, a `.bak` copy and autosave
//! (SPEC.md, "Persistence and file formats").

use std::io::Write;
use std::path::{Path, PathBuf};

use axl_parts::catalog::Catalog;
use axl_parts::types::{ColorId, PartId};
use serde::{Deserialize, Serialize};

use crate::model::{Assembly, DeviceInstance, PlacedPart};
use crate::orient::Orient;

/// The newest `.axlbot` version this build writes and understands.
pub const AXLBOT_VERSION: u32 = 1;

/// Autosave delay after the last edit, seconds.
pub const AUTOSAVE_DELAY_S: f64 = 1.0;

/// Why a robot file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// Disk problem.
    #[error("Could not access {path}: {message}")]
    Io {
        /// Path.
        path: String,
        /// Message.
        message: String,
    },
    /// Not valid RON for a robot.
    #[error("{path} is not a robot file: {message}")]
    Parse {
        /// Path.
        path: String,
        /// Message.
        message: String,
    },
    /// Made by a newer AXL.
    #[error("This file was made by a newer version of AXL.")]
    TooNew,
    /// The file names something that does not exist or is inconsistent.
    #[error("{0}")]
    Invalid(String),
}

#[derive(Serialize, Deserialize)]
struct FileRobot {
    version: u32,
    name: String,
    parts: Vec<FilePart>,
}

#[derive(Serialize, Deserialize)]
struct FilePart {
    uid: u32,
    part: PartId,
    pos: (i32, i32, i32),
    orient: u8,
    color: ColorId,
    device: Option<DeviceInstance>,
}

/// Serialise an assembly as `.axlbot` text.
pub fn to_text(asm: &Assembly) -> String {
    let file = FileRobot {
        version: AXLBOT_VERSION,
        name: asm.name.clone(),
        parts: asm
            .parts
            .iter()
            .map(|p| FilePart {
                uid: p.uid,
                part: p.part.clone(),
                pos: (p.pos[0], p.pos[1], p.pos[2]),
                orient: p.orient.index(),
                color: p.color.clone(),
                device: p.device.clone(),
            })
            .collect(),
    };
    let cfg = ron::ser::PrettyConfig::new().indentor("    ");
    let mut s = ron::ser::to_string_pretty(&file, cfg).unwrap_or_default();
    s.push('\n');
    s
}

/// Parse `.axlbot` text. Checks versions, parts and orientations; never panics.
pub fn from_text(text: &str, cat: &Catalog, path: &str) -> Result<Assembly, FileError> {
    let file: FileRobot = ron::de::from_str(text).map_err(|e| FileError::Parse {
        path: path.into(),
        message: e.to_string(),
    })?;
    if file.version > AXLBOT_VERSION {
        return Err(FileError::TooNew);
    }
    // Older versions would be migrated here; version 1 is the first.
    let mut parts = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for p in file.parts {
        if cat.get(&p.part.0).is_none() {
            return Err(FileError::Invalid(format!("This robot uses a part that does not exist: {}.", p.part.0)));
        }
        let Some(orient) = Orient::from_index(p.orient) else {
            return Err(FileError::Invalid(format!("Part {} has an orientation outside 0 to 23.", p.uid)));
        };
        if !seen.insert(p.uid) {
            return Err(FileError::Invalid(format!("Two parts have the uid {}.", p.uid)));
        }
        parts.push(PlacedPart {
            uid: p.uid,
            part: p.part,
            pos: [p.pos.0, p.pos.1, p.pos.2],
            orient,
            color: p.color,
            device: p.device,
        });
    }
    let next_uid = parts.iter().map(|p| p.uid).max().map_or(1, |m| m + 1);
    Ok(Assembly { name: file.name, parts, next_uid })
}

fn io_err(path: &Path, e: std::io::Error) -> FileError {
    FileError::Io { path: path.display().to_string(), message: e.to_string() }
}

/// Write `bytes` to `path` atomically: a temporary file in the same folder, then a rename. An
/// existing file is first copied to `<path>.bak`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| io_err(&tmp, e))?;
        f.write_all(bytes).map_err(|e| io_err(&tmp, e))?;
        f.sync_all().map_err(|e| io_err(&tmp, e))?;
    }
    if path.exists() {
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        std::fs::copy(path, PathBuf::from(bak)).map_err(|e| io_err(path, e))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| io_err(path, e))
}

/// Save a robot.
pub fn save(asm: &Assembly, path: &Path) -> Result<(), FileError> {
    write_atomic(path, to_text(asm).as_bytes())
}

/// Load a robot.
pub fn load(path: &Path, cat: &Catalog) -> Result<Assembly, FileError> {
    let text = std::fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    from_text(&text, cat, &path.display().to_string())
}

/// The user data folder: `AXL_DATA_DIR` if set, else the platform folder.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("AXL_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("org", "axl", "AXL").map(|d| d.data_dir().to_path_buf())
}

/// `<data>/robots`.
pub fn robots_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("robots"))
}

/// A safe file stem for a robot name.
pub fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "robot".into() } else { s }
}

/// Where a robot with this name is saved.
pub fn robot_path(name: &str) -> Option<PathBuf> {
    robots_dir().map(|d| d.join(format!("{}.axlbot", file_stem(name))))
}

/// Saved robots in `<data>/robots`, sorted by file name.
pub fn list_robots() -> Vec<PathBuf> {
    let Some(dir) = robots_dir() else { return vec![] };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "axlbot"))
        .collect();
    v.sort();
    v
}

/// Decides when to autosave: one second after the last edit.
#[derive(Clone, Debug, Default)]
pub struct Autosave {
    seen_rev: u64,
    saved_rev: u64,
    last_edit_s: f64,
}

impl Autosave {
    /// Call every frame with the clock (seconds) and the builder's revision. Returns true when the
    /// robot should be written now; then call [`Autosave::saved`].
    pub fn poll(&mut self, now_s: f64, rev: u64) -> bool {
        if rev != self.seen_rev {
            self.seen_rev = rev;
            self.last_edit_s = now_s;
        }
        rev != self.saved_rev && now_s - self.last_edit_s >= AUTOSAVE_DELAY_S
    }

    /// Record that revision `rev` is on disk.
    pub fn saved(&mut self, rev: u64) {
        self.saved_rev = rev;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_waits_one_second_after_the_last_edit() {
        let mut a = Autosave::default();
        assert!(!a.poll(0.0, 0));
        assert!(!a.poll(10.0, 1)); // edit seen at t = 10
        assert!(!a.poll(10.9, 1));
        assert!(!a.poll(11.5, 2)); // another edit restarts the wait
        assert!(!a.poll(12.4, 2));
        assert!(a.poll(12.5, 2));
        a.saved(2);
        assert!(!a.poll(20.0, 2));
    }

    #[test]
    fn stems_are_safe() {
        assert_eq!(file_stem("My Rover/2"), "My_Rover_2");
        assert_eq!(file_stem("///"), "robot");
    }
}
