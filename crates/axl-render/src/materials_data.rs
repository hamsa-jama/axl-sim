//! Colours, per-part variation and the micro-surface normal map.
//!
//! This module is plain data and arithmetic (no GPU), so it can be tested directly.

/// The nine part colours from the spec, as sRGB bytes.
pub const PART_COLORS: [(&str, [u8; 3]); 9] = [
    ("white", [0xF2, 0xF3, 0xF2]),
    ("light_grey", [0xA0, 0xA5, 0xA9]),
    ("dark_grey", [0x6C, 0x6E, 0x68]),
    ("black", [0x1B, 0x2A, 0x34]),
    ("red", [0xC9, 0x1A, 0x09]),
    ("blue", [0x00, 0x55, 0xBF]),
    ("yellow", [0xF2, 0xCD, 0x37]),
    ("green", [0x23, 0x78, 0x41]),
    ("orange", [0xFE, 0x8A, 0x18]),
];

/// Look up a colour by its id (for example `"red"`).
pub fn part_color(id: &str) -> Option<[u8; 3]> {
    PART_COLORS.iter().find(|(n, _)| *n == id).map(|(_, c)| *c)
}

/// ABS plastic surface values (SPEC.md, "Materials").
pub mod abs {
    /// Perceptual roughness.
    pub const ROUGHNESS: f32 = 0.28;
    /// Reflectance.
    pub const REFLECTANCE: f32 = 0.5;
    /// Clearcoat strength.
    pub const CLEARCOAT: f32 = 0.15;
    /// Clearcoat roughness.
    pub const CLEARCOAT_ROUGHNESS: f32 = 0.2;
}

/// Maximum roughness change from the per-part variation.
pub const ROUGHNESS_JITTER: f32 = 0.03;
/// Maximum brightness change from the per-part variation (1.5%).
pub const BRIGHTNESS_JITTER: f32 = 0.015;
/// Strength of the micro-surface normal map.
pub const MICRO_NORMAL_STRENGTH: f32 = 0.025;
/// Side of the micro-surface texture in pixels.
pub const MICRO_NORMAL_SIZE: usize = 256;

/// Small, fast, well-mixed hash (SplitMix64 finaliser).
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A number in `[-1, 1]` derived from `uid` and a `salt`.
fn signed_unit(uid: u32, salt: u64) -> f32 {
    let h = splitmix64((uid as u64) ^ (salt << 32));
    // Top 24 bits give an exact f32 fraction in [0, 1).
    let f = (h >> 40) as f32 / (1u32 << 24) as f32;
    f * 2.0 - 1.0
}

/// Per-part variation, seeded by the part's `uid`:
/// roughness +-0.03 and brightness +-1.5%.
pub fn part_variation(uid: u32) -> PartVariation {
    PartVariation {
        roughness_delta: signed_unit(uid, 1) * ROUGHNESS_JITTER,
        brightness_scale: 1.0 + signed_unit(uid, 2) * BRIGHTNESS_JITTER,
    }
}

/// The result of [`part_variation`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartVariation {
    /// Add to the roughness.
    pub roughness_delta: f32,
    /// Multiply the base colour by this.
    pub brightness_scale: f32,
}

/// Encode an `f32` as IEEE half-precision bits (round to nearest even).
/// Used for the procedural environment cubemap, which is `Rgba16Float`.
pub fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x007F_FFFF;
    if exp == 0xFF {
        // Infinity or NaN.
        return sign | 0x7C00 | if mant != 0 { 0x0200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7C00; // overflow to infinity
    }
    if e <= 0 {
        if e < -10 {
            return sign; // too small: zero
        }
        // Subnormal half.
        let m = mant | 0x0080_0000;
        let shift = (14 - e) as u32;
        let half = (m >> shift) as u16;
        let round_bit = (m >> (shift - 1)) & 1;
        let sticky = m & ((1 << (shift - 1)) - 1);
        let mut out = sign | half;
        if round_bit == 1 && (sticky != 0 || (half & 1) == 1) {
            out += 1;
        }
        return out;
    }
    let half = sign | ((e as u16) << 10) | (mant >> 13) as u16;
    let round_bit = (mant >> 12) & 1;
    let sticky = mant & 0x0FFF;
    if round_bit == 1 && (sticky != 0 || (half & 1) == 1) {
        half + 1
    } else {
        half
    }
}

/// A tileable value-noise height field in `[0, 1]`, `size` x `size`.
fn tileable_height(size: usize, seed: u64) -> Vec<f32> {
    // Sum of three octaves of lattice noise; every lattice wraps, so the result tiles.
    let octaves: [(usize, f32); 3] = [(8, 0.5), (16, 0.3), (32, 0.2)];
    let mut h = vec![0.0f32; size * size];
    for (oi, (cells, weight)) in octaves.iter().enumerate() {
        let lattice: Vec<f32> = (0..cells * cells)
            .map(|i| {
                let v = splitmix64(seed ^ ((oi as u64) << 48) ^ i as u64);
                (v >> 40) as f32 / (1u32 << 24) as f32
            })
            .collect();
        for y in 0..size {
            for x in 0..size {
                let fx = x as f32 / size as f32 * *cells as f32;
                let fy = y as f32 / size as f32 * *cells as f32;
                let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
                let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
                let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
                let at = |ix: usize, iy: usize| lattice[(iy % cells) * cells + (ix % cells)];
                let top = at(x0, y0) * (1.0 - sx) + at(x0 + 1, y0) * sx;
                let bot = at(x0, y0 + 1) * (1.0 - sx) + at(x0 + 1, y0 + 1) * sx;
                h[y * size + x] += (top * (1.0 - sy) + bot * sy) * weight;
            }
        }
    }
    h
}

/// A tangent-space normal map (RGBA8, linear) of fine moulded-plastic grain.
///
/// `strength` scales how far the normals lean away from straight up
/// (0 is flat, the spec uses 0.15). The texture tiles seamlessly.
pub fn micro_surface_normal_map(size: usize, strength: f32, seed: u64) -> Vec<u8> {
    let h = tileable_height(size, seed);
    let at = |x: isize, y: isize| {
        let xi = x.rem_euclid(size as isize) as usize;
        let yi = y.rem_euclid(size as isize) as usize;
        h[yi * size + xi]
    };
    let mut out = Vec::with_capacity(size * size * 4);
    for y in 0..size as isize {
        for x in 0..size as isize {
            // Central differences, scaled so a full-range ramp over the texture leans by `strength`.
            let dx = (at(x + 1, y) - at(x - 1, y)) * 0.5 * size as f32 / 24.0;
            let dy = (at(x, y + 1) - at(x, y - 1)) * 0.5 * size as f32 / 24.0;
            let n = [-dx * strength, -dy * strength, 1.0];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            for c in n {
                let v = c / len * 0.5 + 0.5;
                out.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
            }
            out.push(255);
        }
    }
    out
}

/// Number of mip levels for a square texture of side `size`.
pub fn mip_count(size: usize) -> u32 {
    usize::BITS - size.max(1).leading_zeros()
}

/// Downsample a tangent-space normal map (RGBA8) one level by averaging 2x2 blocks and renormalising.
fn downsample_normals(src: &[u8], size: usize) -> Vec<u8> {
    let half = (size / 2).max(1);
    let mut out = Vec::with_capacity(half * half * 4);
    for y in 0..half {
        for x in 0..half {
            let mut n = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(size - 1);
                let sy = (y * 2 + dy).min(size - 1);
                let i = (sy * size + sx) * 4;
                for c in 0..3 {
                    n[c] += src[i + c] as f32 / 255.0 * 2.0 - 1.0;
                }
            }
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            for c in n {
                out.push(((c / len) * 0.5 * 255.0 + 127.5).round().clamp(0.0, 255.0) as u8);
            }
            out.push(255);
        }
    }
    out
}

/// The micro-surface normal map with its full mip chain (level 0 first), RGBA8.
pub fn micro_surface_normal_map_mips(size: usize, strength: f32, seed: u64) -> Vec<u8> {
    let mut level = micro_surface_normal_map(size, strength, seed);
    let mut all = level.clone();
    let mut s = size;
    while s > 1 {
        level = downsample_normals(&level, s);
        s /= 2;
        all.extend_from_slice(&level);
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_has_every_level() {
        assert_eq!(mip_count(256), 9);
        assert_eq!(mip_count(1), 1);
        let all = micro_surface_normal_map_mips(16, 0.15, 3);
        // 16x16 + 8x8 + 4x4 + 2x2 + 1x1 pixels, 4 bytes each.
        assert_eq!(all.len(), (256 + 64 + 16 + 4 + 1) * 4);
    }

    #[test]
    fn colours_match_spec() {
        assert_eq!(PART_COLORS.len(), 9);
        assert_eq!(part_color("white"), Some([0xF2, 0xF3, 0xF2]));
        assert_eq!(part_color("black"), Some([0x1B, 0x2A, 0x34]));
        assert_eq!(part_color("orange"), Some([0xFE, 0x8A, 0x18]));
        assert_eq!(part_color("magenta"), None);
    }

    #[test]
    fn variation_is_deterministic_and_bounded() {
        for uid in 0..5000u32 {
            let a = part_variation(uid);
            assert_eq!(a, part_variation(uid));
            assert!(a.roughness_delta.abs() <= ROUGHNESS_JITTER + 1e-6);
            assert!((a.brightness_scale - 1.0).abs() <= BRIGHTNESS_JITTER + 1e-6);
        }
    }

    #[test]
    fn variation_actually_varies() {
        let distinct: std::collections::BTreeSet<u32> = (0..200u32)
            .map(|u| (part_variation(u).roughness_delta * 1e6) as i32 as u32)
            .collect();
        assert!(distinct.len() > 150);
    }

    #[test]
    fn half_float_encoding() {
        assert_eq!(f32_to_f16_bits(0.0), 0x0000);
        assert_eq!(f32_to_f16_bits(1.0), 0x3C00);
        assert_eq!(f32_to_f16_bits(-2.0), 0xC000);
        assert_eq!(f32_to_f16_bits(0.5), 0x3800);
        assert_eq!(f32_to_f16_bits(65504.0), 0x7BFF);
        assert_eq!(f32_to_f16_bits(1.0e9), 0x7C00);
        assert_eq!(f32_to_f16_bits(f32::INFINITY), 0x7C00);
        // Smallest positive subnormal half is 2^-24.
        assert_eq!(f32_to_f16_bits(2.0f32.powi(-24)), 0x0001);
        // 1/3 rounds to 0x3555.
        assert_eq!(f32_to_f16_bits(1.0 / 3.0), 0x3555);
    }

    #[test]
    fn normal_map_is_unit_length_and_leans_by_little() {
        let size = 64;
        let px = micro_surface_normal_map(size, MICRO_NORMAL_STRENGTH, 7);
        assert_eq!(px.len(), size * size * 4);
        let mut max_lean = 0.0f32;
        for p in px.chunks(4) {
            let n: Vec<f32> = p[..3]
                .iter()
                .map(|&b| b as f32 / 255.0 * 2.0 - 1.0)
                .collect();
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 0.02, "len {len}");
            max_lean = max_lean.max((n[0] * n[0] + n[1] * n[1]).sqrt());
            assert_eq!(p[3], 255);
        }
        // Non-trivial but gentle.
        assert!(max_lean > 0.01, "too flat: {max_lean}");
        assert!(max_lean < 0.5, "too rough: {max_lean}");
    }

    #[test]
    fn normal_map_tiles() {
        // The wrap-around difference means opposite edges are continuous.
        let size = 32;
        let a = micro_surface_normal_map(size, 0.15, 1);
        let b = micro_surface_normal_map(size, 0.15, 1);
        assert_eq!(a, b);
        let flat = micro_surface_normal_map(size, 0.0, 1);
        assert!(
            flat.chunks(4)
                .all(|p| p[0] == 128 && p[1] == 128 && p[2] == 255)
        );
    }
}
