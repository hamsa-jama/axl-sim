//! Quality presets and the first-launch benchmark that picks one.
//!
//! The table of values comes straight from the "Quality presets" table in
//! `SPEC.md`. [`QualitySettings`] is plain data so it can be tested without a GPU;
//! `camera_rig` applies it to the camera and lights.

use serde::{Deserialize, Serialize};

/// One of the four presets in the spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum QualityPreset {
    /// 30 fps on integrated graphics.
    Low,
    /// 60 fps at 1080p on an entry GPU.
    Medium,
    /// 60 fps at 1080p on a mid-range GPU.
    #[default]
    High,
    /// 60 fps at 1440p on a high-end GPU.
    Ultra,
}

impl QualityPreset {
    /// All presets, lowest first.
    pub const ALL: [QualityPreset; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];

    /// Name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }

    /// Parse a name such as `high` (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.label().eq_ignore_ascii_case(s.trim()))
    }

    /// The values this preset stands for.
    pub fn settings(self) -> QualitySettings {
        use AaMode::*;
        use AoLevel::*;
        match self {
            Self::Low => QualitySettings {
                render_scale: 0.75,
                aa: Fxaa,
                shadows: ShadowSettings {
                    cascades: 1,
                    map_size: 1024,
                    soft: false,
                },
                contact_shadows: false,
                ao: Off,
                bloom: false,
                micro_surface: false,
            },
            Self::Medium => QualitySettings {
                render_scale: 1.0,
                aa: Smaa,
                shadows: ShadowSettings {
                    cascades: 2,
                    map_size: 2048,
                    soft: false,
                },
                contact_shadows: false,
                ao: Low,
                bloom: true,
                micro_surface: true,
            },
            Self::High => QualitySettings {
                render_scale: 1.0,
                aa: Taa,
                shadows: ShadowSettings {
                    cascades: 4,
                    map_size: 2048,
                    soft: true,
                },
                contact_shadows: true,
                ao: Medium,
                bloom: true,
                micro_surface: true,
            },
            Self::Ultra => QualitySettings {
                render_scale: 1.0,
                aa: Taa,
                shadows: ShadowSettings {
                    cascades: 4,
                    map_size: 4096,
                    soft: true,
                },
                contact_shadows: true,
                ao: High,
                bloom: true,
                micro_surface: true,
            },
        }
    }
}

/// Anti-aliasing technique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AaMode {
    /// Fast approximate AA.
    Fxaa,
    /// Subpixel morphological AA.
    Smaa,
    /// Temporal AA.
    Taa,
}

/// Ambient occlusion level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AoLevel {
    /// No ambient occlusion.
    Off,
    /// Cheapest setting.
    Low,
    /// Balanced.
    Medium,
    /// Best.
    High,
}

/// Directional-light shadow settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowSettings {
    /// Number of cascades.
    pub cascades: usize,
    /// Width and height of each cascade in pixels.
    pub map_size: u32,
    /// Soft (filtered, temporal) shadow edges.
    pub soft: bool,
}

/// Everything a preset controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualitySettings {
    /// Fraction of the window resolution that the 3D view renders at.
    pub render_scale: f32,
    /// Anti-aliasing technique.
    pub aa: AaMode,
    /// Shadow settings.
    pub shadows: ShadowSettings,
    /// Screen-space contact shadows.
    pub contact_shadows: bool,
    /// Ambient occlusion level.
    pub ao: AoLevel,
    /// Bloom post effect.
    pub bloom: bool,
    /// Procedural micro-surface normal map on plastic.
    pub micro_surface: bool,
}

/// Benchmark thresholds, read from `assets/render/quality.ron`.
///
/// placeholder: calibrate. The cut-offs are average frame times (milliseconds)
/// measured while the benchmark renders the look-dev scene at High.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// File format version.
    pub version: u32,
    /// How long the benchmark measures, in seconds.
    pub duration_s: f32,
    /// Frames ignored at the start (shader compilation, window setup).
    pub warmup_frames: u32,
    /// Average frame time at or below this picks Ultra.
    pub ultra_max_ms: f32,
    /// Average frame time at or below this picks High.
    pub high_max_ms: f32,
    /// Average frame time at or below this picks Medium; slower picks Low.
    pub medium_max_ms: f32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            version: 1,
            duration_s: 3.0,
            warmup_frames: 20,
            ultra_max_ms: 6.0,
            high_max_ms: 12.0,
            medium_max_ms: 20.0,
        }
    }
}

impl BenchmarkConfig {
    /// Pick the preset for a measured average frame time.
    pub fn choose(&self, avg_frame_ms: f32) -> QualityPreset {
        if avg_frame_ms <= self.ultra_max_ms {
            QualityPreset::Ultra
        } else if avg_frame_ms <= self.high_max_ms {
            QualityPreset::High
        } else if avg_frame_ms <= self.medium_max_ms {
            QualityPreset::Medium
        } else {
            QualityPreset::Low
        }
    }
}

/// Accumulates frame times for the first-launch benchmark.
#[derive(Clone, Debug)]
pub struct BenchmarkRun {
    config: BenchmarkConfig,
    frames_seen: u32,
    elapsed_s: f32,
    measured_s: f32,
    measured_frames: u32,
}

impl BenchmarkRun {
    /// Start a run.
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            frames_seen: 0,
            elapsed_s: 0.0,
            measured_s: 0.0,
            measured_frames: 0,
        }
    }

    /// Record one frame. Returns the chosen preset and average frame time
    /// once the measuring window has passed.
    pub fn push_frame(&mut self, dt_s: f32) -> Option<(QualityPreset, f32)> {
        self.frames_seen += 1;
        if self.frames_seen <= self.config.warmup_frames {
            return None;
        }
        self.elapsed_s += dt_s;
        self.measured_s += dt_s;
        self.measured_frames += 1;
        if self.elapsed_s >= self.config.duration_s && self.measured_frames > 0 {
            let avg_ms = self.measured_s / self.measured_frames as f32 * 1000.0;
            Some((self.config.choose(avg_ms), avg_ms))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_match_spec_table() {
        let low = QualityPreset::Low.settings();
        assert_eq!(low.render_scale, 0.75);
        assert_eq!(low.aa, AaMode::Fxaa);
        assert_eq!(low.shadows.cascades, 1);
        assert_eq!(low.shadows.map_size, 1024);
        assert!(!low.contact_shadows && !low.bloom && !low.micro_surface);
        assert_eq!(low.ao, AoLevel::Off);

        let med = QualityPreset::Medium.settings();
        assert_eq!(med.aa, AaMode::Smaa);
        assert_eq!((med.shadows.cascades, med.shadows.map_size), (2, 2048));
        assert!(!med.contact_shadows && med.bloom && med.micro_surface);
        assert_eq!(med.ao, AoLevel::Low);

        let high = QualityPreset::High.settings();
        assert_eq!(high.aa, AaMode::Taa);
        assert_eq!((high.shadows.cascades, high.shadows.map_size), (4, 2048));
        assert!(high.shadows.soft && high.contact_shadows);
        assert_eq!(high.ao, AoLevel::Medium);

        let ultra = QualityPreset::Ultra.settings();
        assert_eq!(ultra.shadows.map_size, 4096);
        assert_eq!(ultra.ao, AoLevel::High);
        assert_eq!(ultra.render_scale, 1.0);
    }

    #[test]
    fn parse_names() {
        assert_eq!(QualityPreset::parse("HIGH"), Some(QualityPreset::High));
        assert_eq!(QualityPreset::parse(" low "), Some(QualityPreset::Low));
        assert_eq!(QualityPreset::parse("max"), None);
    }

    #[test]
    fn benchmark_thresholds() {
        let c = BenchmarkConfig::default();
        assert_eq!(c.choose(4.0), QualityPreset::Ultra);
        assert_eq!(c.choose(10.0), QualityPreset::High);
        assert_eq!(c.choose(16.0), QualityPreset::Medium);
        assert_eq!(c.choose(50.0), QualityPreset::Low);
    }

    #[test]
    fn benchmark_run_skips_warmup_and_finishes_after_duration() {
        let mut run = BenchmarkRun::new(BenchmarkConfig::default());
        // Warm-up frames are slow and must not count.
        for _ in 0..20 {
            assert!(run.push_frame(0.5).is_none());
        }
        let mut result = None;
        for _ in 0..1000 {
            if let Some(r) = run.push_frame(0.010) {
                result = Some(r);
                break;
            }
        }
        let (preset, avg) = result.expect("benchmark finishes");
        assert!((avg - 10.0).abs() < 0.01);
        assert_eq!(preset, QualityPreset::High);
    }
}
