//! Grid-floor rules from the spec, as plain functions.
//!
//! The WESL shader in `shaders/grid_floor.wesl` repeats the same formulas
//! (look for the "mirrors grid_math" comments), so these tests also pin down
//! what the shader must do.

use serde::{Deserialize, Serialize};

/// Which unit system the grid lines follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GridUnits {
    /// Minor 8 mm (1 stud), major 80 mm (10 studs), bold 1 m.
    #[default]
    Studs,
    /// Minor 10 mm, major 100 mm, bold 1 m.
    Metric,
}

impl GridUnits {
    /// Line spacings in metres: `[minor, major, bold]`.
    pub fn spacings(self) -> [f32; 3] {
        match self {
            Self::Studs => [0.008, 0.080, 1.0],
            Self::Metric => [0.010, 0.100, 1.0],
        }
    }

    /// The other system.
    pub fn toggled(self) -> Self {
        match self {
            Self::Studs => Self::Metric,
            Self::Metric => Self::Studs,
        }
    }
}

/// Lines closer together than this many pixels fade out completely.
pub const MIN_LINE_SPACING_PX: f32 = 6.0;

/// Distance (m) at which lines start to fade, and where they are gone.
pub const FADE_NEAR_M: f32 = 4.0;
/// Distance (m) at which every line has faded out.
pub const FADE_FAR_M: f32 = 40.0;

/// Default grid opacity in the builder (60%).
pub const OPACITY_BUILDER: f32 = 0.60;
/// Default grid opacity while simulating (25%).
pub const OPACITY_SIMULATE: f32 = 0.25;

/// `smoothstep` as in WGSL.
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How visible one line family is, given the on-screen distance between its lines.
///
/// 0 at or below [`MIN_LINE_SPACING_PX`], full at twice that. Mirrors `spacing_fade` in the shader.
pub fn spacing_fade(spacing_px: f32) -> f32 {
    smoothstep(MIN_LINE_SPACING_PX, 2.0 * MIN_LINE_SPACING_PX, spacing_px)
}

/// Fade of every line with distance from the camera. Mirrors `distance_fade` in the shader.
pub fn distance_fade(distance_m: f32) -> f32 {
    1.0 - smoothstep(FADE_NEAR_M, FADE_FAR_M, distance_m)
}

/// Anti-aliased coverage of a line of `width_px` pixels, for a pixel whose centre
/// is `dist_px` away from the line centre. Mirrors `line_coverage` in the shader.
pub fn line_coverage(dist_px: f32, width_px: f32) -> f32 {
    (0.5 + 0.5 * width_px - dist_px).clamp(0.0, 1.0)
}

/// Pixels between neighbouring lines of spacing `spacing_m` when one pixel covers `metres_per_pixel`.
pub fn spacing_in_pixels(spacing_m: f32, metres_per_pixel: f32) -> f32 {
    if metres_per_pixel <= 0.0 {
        f32::INFINITY
    } else {
        spacing_m / metres_per_pixel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacings_match_spec() {
        assert_eq!(GridUnits::Studs.spacings(), [0.008, 0.08, 1.0]);
        assert_eq!(GridUnits::Metric.spacings(), [0.01, 0.1, 1.0]);
        assert_eq!(GridUnits::Studs.toggled(), GridUnits::Metric);
    }

    #[test]
    fn lines_vanish_below_six_pixels() {
        assert_eq!(spacing_fade(0.0), 0.0);
        assert_eq!(spacing_fade(5.99), 0.0);
        assert_eq!(spacing_fade(6.0), 0.0);
        assert!(spacing_fade(8.0) > 0.0 && spacing_fade(8.0) < 1.0);
        assert_eq!(spacing_fade(12.0), 1.0);
        assert_eq!(spacing_fade(500.0), 1.0);
    }

    #[test]
    fn spacing_fade_is_monotonic() {
        let mut prev = 0.0;
        for i in 0..200 {
            let v = spacing_fade(i as f32 * 0.1);
            assert!(v >= prev);
            prev = v;
        }
    }

    #[test]
    fn minor_fades_before_major_when_zooming_out() {
        // At 1 mm per pixel a stud (8 mm) is 8 px apart: still fading in. At
        // 2 mm per pixel it is 4 px apart: gone, while a 10-stud line is 40 px.
        let [minor, major, _] = GridUnits::Studs.spacings();
        assert!(spacing_fade(spacing_in_pixels(minor, 0.002)) == 0.0);
        assert!(spacing_fade(spacing_in_pixels(major, 0.002)) == 1.0);
    }

    #[test]
    fn distance_fade_goes_from_one_to_zero() {
        assert_eq!(distance_fade(0.0), 1.0);
        assert_eq!(distance_fade(FADE_NEAR_M), 1.0);
        assert_eq!(distance_fade(FADE_FAR_M), 0.0);
        assert!(distance_fade(20.0) > 0.0 && distance_fade(20.0) < 1.0);
        assert_eq!(distance_fade(1000.0), 0.0);
    }

    #[test]
    fn line_coverage_is_antialiased() {
        assert_eq!(line_coverage(0.0, 1.0), 1.0);
        assert!((line_coverage(0.5, 1.0) - 0.5).abs() < 1e-6);
        assert_eq!(line_coverage(1.5, 1.0), 0.0);
        assert_eq!(line_coverage(1.0, 3.0), 1.0);
    }

    #[test]
    fn default_opacities_match_spec() {
        assert_eq!(OPACITY_BUILDER, 0.60);
        assert_eq!(OPACITY_SIMULATE, 0.25);
    }
}
