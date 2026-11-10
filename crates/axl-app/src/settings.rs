//! `settings.ron` in the user's data folder: quality, units, grid opacity and camera options.
//!
//! The file is plain RON with a top-level `version`, written atomically (temporary file in the
//! same folder, then rename) with the previous copy kept as `settings.ron.bak`. Milestone M10
//! adds the remaining fields from the spec (UI scale, toolchain paths).

use std::io::Write;
use std::path::{Path, PathBuf};

use axl_render::grid_math::{GridUnits, OPACITY_BUILDER, OPACITY_SIMULATE};
use axl_render::quality::QualityPreset;
use serde::{Deserialize, Serialize};

/// The newest file version this build understands.
pub const SETTINGS_VERSION: u32 = 1;

/// User settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// File format version.
    pub version: u32,
    /// Chosen quality preset; `None` until the first-launch benchmark has run.
    pub quality: Option<QualityPreset>,
    /// Stud or metric grid.
    pub units: GridUnits,
    /// Grid opacity in the builder, 0..=1.
    pub grid_opacity_builder: f32,
    /// Grid opacity while simulating, 0..=1.
    pub grid_opacity_simulate: f32,
    /// Reverse the orbit direction.
    pub invert_orbit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            quality: None,
            units: GridUnits::Studs,
            grid_opacity_builder: OPACITY_BUILDER,
            grid_opacity_simulate: OPACITY_SIMULATE,
            invert_orbit: false,
        }
    }
}

/// Why settings could not be loaded or saved.
#[derive(Debug)]
pub enum SettingsError {
    /// Reading or writing failed.
    Io(std::io::Error),
    /// The file is not valid RON.
    Parse(String),
    /// The file was made by a newer version.
    TooNew,
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(m) => write!(f, "settings.ron is not valid: {m}"),
            Self::TooNew => write!(f, "This file was made by a newer version of AXL."),
        }
    }
}

impl std::error::Error for SettingsError {}

impl Settings {
    /// `<data dir>/settings.ron`, where the data dir comes from the platform (see the spec).
    pub fn default_path() -> Option<PathBuf> {
        // `AXL_DATA_DIR` points the settings at another folder (for example an empty one, to
        // repeat the first-launch benchmark).
        if let Some(dir) = std::env::var_os("AXL_DATA_DIR").filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(dir).join("settings.ron"));
        }
        directories::ProjectDirs::from("org", "axl", "AXL")
            .map(|d| d.data_dir().join("settings.ron"))
    }

    /// Parse RON text. Values are clamped to sane ranges.
    pub fn from_ron(text: &str) -> Result<Self, SettingsError> {
        let mut s: Settings =
            ron::de::from_str(text).map_err(|e| SettingsError::Parse(e.to_string()))?;
        if s.version > SETTINGS_VERSION {
            return Err(SettingsError::TooNew);
        }
        s.grid_opacity_builder = s.grid_opacity_builder.clamp(0.0, 1.0);
        s.grid_opacity_simulate = s.grid_opacity_simulate.clamp(0.0, 1.0);
        s.version = SETTINGS_VERSION;
        Ok(s)
    }

    /// Serialise to RON text.
    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .unwrap_or_else(|_| String::from("(version: 1)"))
    }

    /// Load from `path`. A missing file gives the defaults; a broken or newer file is an error
    /// (the caller shows it and carries on with defaults without overwriting the file).
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_ron(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(SettingsError::Io(e)),
        }
    }

    /// Write atomically, keeping the previous copy as `<file>.bak`.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        write_atomic(path, self.to_ron().as_bytes()).map_err(SettingsError::Io)
    }
}

/// Write `bytes` to `path` through a temporary file in the same folder, keeping a `.bak` copy.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if path.exists() {
        let bak = dir.join(format!("{name}.bak"));
        // A failed backup must not stop the save.
        let _ = std::fs::copy(path, bak);
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("axl-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn defaults_match_spec() {
        let s = Settings::default();
        assert_eq!(s.units, GridUnits::Studs);
        assert_eq!(s.grid_opacity_builder, 0.60);
        assert_eq!(s.grid_opacity_simulate, 0.25);
        assert_eq!(s.quality, None);
    }

    #[test]
    fn round_trip() {
        let s = Settings {
            quality: Some(QualityPreset::Medium),
            units: GridUnits::Metric,
            grid_opacity_builder: 0.3,
            invert_orbit: true,
            ..Settings::default()
        };
        assert_eq!(Settings::from_ron(&s.to_ron()).unwrap(), s);
    }

    #[test]
    fn newer_version_is_refused_and_bad_text_is_an_error() {
        assert!(matches!(
            Settings::from_ron("(version: 7)"),
            Err(SettingsError::TooNew)
        ));
        assert!(matches!(
            Settings::from_ron("not ron at all"),
            Err(SettingsError::Parse(_))
        ));
    }

    #[test]
    fn partial_files_fill_in_defaults_and_values_are_clamped() {
        let s = Settings::from_ron("(version: 1, grid_opacity_builder: 5.0)").unwrap();
        assert_eq!(s.grid_opacity_builder, 1.0);
        assert_eq!(s.grid_opacity_simulate, 0.25);
    }

    #[test]
    fn save_is_atomic_and_keeps_a_backup() {
        let dir = scratch("save");
        let path = dir.join("settings.ron");
        let mut s = Settings::default();
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), s);
        s.units = GridUnits::Metric;
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().units, GridUnits::Metric);
        let bak = Settings::load(&dir.join("settings.ron.bak")).unwrap();
        assert_eq!(bak.units, GridUnits::Studs);
        // No temporary file is left behind.
        assert!(!dir.join(".settings.ron.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn benchmark_choice_is_persisted_to_a_fresh_data_folder() {
        use axl_render::quality::{BenchmarkConfig, BenchmarkRun};
        // A slow integrated GPU: 80 ms frames. The benchmark must pick Low.
        let mut run = BenchmarkRun::new(BenchmarkConfig::default());
        let picked = (0..2000)
            .find_map(|_| run.push_frame(0.080))
            .expect("benchmark finishes");
        assert_eq!(picked.0, QualityPreset::Low);
        // The data folder does not exist yet; saving creates it.
        let dir = scratch("bench").join("nested").join("axl");
        let path = dir.join("settings.ron");
        let s = Settings {
            quality: Some(picked.0),
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert!(path.exists());
        assert_eq!(
            Settings::load(&path).unwrap().quality,
            Some(QualityPreset::Low)
        );
        let _ = std::fs::remove_dir_all(scratch("bench"));
    }

    #[test]
    fn an_existing_data_folder_does_not_skip_the_benchmark() {
        // The Code screen creates projects/ in the data folder; settings.ron is still missing.
        let dir = scratch("existing");
        std::fs::create_dir_all(dir.join("projects/m01/rust")).unwrap();
        assert_eq!(
            Settings::load(&dir.join("settings.ron")).unwrap().quality,
            None
        );
        // A settings file without a saved quality also still benchmarks.
        let mut s = Settings::default();
        s.units = GridUnits::Metric;
        s.save(&dir.join("settings.ron")).unwrap();
        assert_eq!(
            Settings::load(&dir.join("settings.ron")).unwrap().quality,
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = scratch("missing");
        assert_eq!(
            Settings::load(&dir.join("nope.ron")).unwrap(),
            Settings::default()
        );
    }
}
