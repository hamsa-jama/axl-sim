//! Optional studio HDRI: if `assets/render/studio.hdr` exists it replaces the procedural studio.
//!
//! The file is a Radiance RGBE image (the `.hdr` download from Poly Haven), equirectangular.
//! It is decoded here (flat and RLE scanlines) and resampled into a cubemap, because the
//! runtime environment filter wants a cube.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
    TextureViewDimension,
};

use crate::lighting::cube_texel_direction;
use crate::materials_data::f32_to_f16_bits;

/// A decoded equirectangular image, linear RGB, row 0 at the top.
pub struct Equirect {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height` linear RGB pixels.
    pub pixels: Vec<[f32; 3]>,
}

/// Decode a Radiance `.hdr` file (`-Y h +X w` orientation only, flat or new-style RLE).
pub fn decode_radiance(bytes: &[u8]) -> Result<Equirect, String> {
    let mut pos = 0;
    let line = |pos: &mut usize| -> Option<&[u8]> {
        let start = *pos;
        let nl = bytes[start..].iter().position(|&b| b == b'\n')?;
        *pos = start + nl + 1;
        Some(&bytes[start..start + nl])
    };
    let first = line(&mut pos).ok_or("empty file")?;
    if !first.starts_with(b"#?") {
        return Err("not a Radiance HDR file (missing #? header)".into());
    }
    // Header lines until the blank line.
    loop {
        let l = line(&mut pos).ok_or("truncated header")?;
        if l.is_empty() {
            break;
        }
    }
    let res = line(&mut pos).ok_or("missing resolution line")?;
    let res = std::str::from_utf8(res).map_err(|_| "bad resolution line")?;
    let parts: Vec<&str> = res.split_whitespace().collect();
    if parts.len() != 4 || parts[0] != "-Y" || parts[2] != "+X" {
        return Err(format!(
            "unsupported orientation {res:?}; expected -Y h +X w"
        ));
    }
    let height: usize = parts[1].parse().map_err(|_| "bad height")?;
    let width: usize = parts[3].parse().map_err(|_| "bad width")?;
    if width == 0 || height == 0 || width > 16384 || height > 8192 {
        return Err("unreasonable image size".into());
    }
    let mut pixels = Vec::with_capacity(width * height);
    let mut row = vec![[0u8; 4]; width];
    for _ in 0..height {
        let rle = (8..32768).contains(&width)
            && bytes.len() >= pos + 4
            && bytes[pos] == 2
            && bytes[pos + 1] == 2
            && (bytes[pos + 2] as usize) << 8 | bytes[pos + 3] as usize == width;
        if rle {
            pos += 4;
            for ch in 0..4 {
                let mut x = 0;
                while x < width {
                    let n = *bytes.get(pos).ok_or("truncated data")? as usize;
                    pos += 1;
                    if n > 128 {
                        let run = n - 128;
                        let v = *bytes.get(pos).ok_or("truncated data")?;
                        pos += 1;
                        if x + run > width {
                            return Err("bad RLE run".into());
                        }
                        for p in row.iter_mut().skip(x).take(run) {
                            p[ch] = v;
                        }
                        x += run;
                    } else {
                        if n == 0 || x + n > width || pos + n > bytes.len() {
                            return Err("bad RLE literal".into());
                        }
                        for i in 0..n {
                            row[x + i][ch] = bytes[pos + i];
                        }
                        pos += n;
                        x += n;
                    }
                }
            }
        } else {
            if pos + width * 4 > bytes.len() {
                return Err("truncated data".into());
            }
            for (x, p) in row.iter_mut().enumerate() {
                p.copy_from_slice(&bytes[pos + x * 4..pos + x * 4 + 4]);
            }
            pos += width * 4;
        }
        pixels.extend(row.iter().map(|p| rgbe_to_linear(*p)));
    }
    Ok(Equirect {
        width,
        height,
        pixels,
    })
}

fn rgbe_to_linear(p: [u8; 4]) -> [f32; 3] {
    if p[3] == 0 {
        return [0.0; 3];
    }
    let f = (2.0f32).powi(p[3] as i32 - 136);
    [p[0] as f32 * f, p[1] as f32 * f, p[2] as f32 * f]
}

impl Equirect {
    /// Bilinear sample along a Bevy-world direction (+Y up). Longitude 0 (image centre) faces
    /// AXL +X, and longitude grows towards AXL +Y.
    pub fn sample(&self, dir: Vec3) -> Vec3 {
        let d = dir.normalize_or_zero();
        // Bevy (x, y, z) is AXL (x, -z, y).
        let (ax, ay) = (d.x, -d.z);
        let lon = ay.atan2(ax); // -pi..pi
        let lat = d.y.clamp(-1.0, 1.0).asin();
        let u = (0.5 + lon / std::f32::consts::TAU) * self.width as f32 - 0.5;
        let v = (0.5 - lat / std::f32::consts::PI) * self.height as f32 - 0.5;
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let at = |x: f32, y: f32| {
            let xi = (x as i64).rem_euclid(self.width as i64) as usize;
            let yi = (y as i64).clamp(0, self.height as i64 - 1) as usize;
            Vec3::from(self.pixels[yi * self.width + xi])
        };
        let top = at(x0, y0).lerp(at(x0 + 1.0, y0), fx);
        let bot = at(x0, y0 + 1.0).lerp(at(x0 + 1.0, y0 + 1.0), fx);
        top.lerp(bot, fy)
    }
}

/// Resample an equirectangular image into an `Rgba16Float` cubemap of `size` x `size` x 6.
/// Values are clamped to the half-float range.
pub fn equirect_to_cubemap(src: &Equirect, size: usize) -> Image {
    let mut data: Vec<u8> = Vec::with_capacity(size * size * 6 * 8);
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let c = src.sample(cube_texel_direction(face, x, y, size));
                for v in [c.x, c.y, c.z, 1.0] {
                    data.extend_from_slice(&f32_to_f16_bits(v.min(60000.0)).to_le_bytes());
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..Default::default()
    });
    image
}

/// The studio cubemap: `assets/render/studio.hdr` if present and valid, else the procedural one.
pub fn load_studio_cubemap(size: usize) -> Image {
    let path = crate::lighting::assets_dir().join("render/studio.hdr");
    match std::fs::read(&path) {
        Ok(bytes) => match decode_radiance(&bytes) {
            Ok(img) => {
                tracing::info!(
                    "studio environment: {} ({}x{})",
                    path.display(),
                    img.width,
                    img.height
                );
                return equirect_to_cubemap(&img, size);
            }
            Err(e) => tracing::warn!("{}: {e}; using the procedural studio", path.display()),
        },
        Err(_) => tracing::debug!("no {}; using the procedural studio", path.display()),
    }
    crate::lighting::studio_cubemap(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: usize, h: usize) -> Vec<u8> {
        format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {h} +X {w}\n").into_bytes()
    }

    #[test]
    fn decodes_flat_pixels() {
        let mut f = header(2, 1);
        // 128 * 2^(129-136) = 1.0 ; second pixel black.
        f.extend_from_slice(&[128, 128, 128, 129, 0, 0, 0, 0]);
        let img = decode_radiance(&f).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert!((img.pixels[0][0] - 1.0).abs() < 1e-6);
        assert_eq!(img.pixels[1], [0.0; 3]);
    }

    #[test]
    fn decodes_rle_scanline() {
        let w = 8;
        let mut f = header(w, 1);
        f.extend_from_slice(&[2, 2, 0, w as u8]);
        for ch_val in [64u8, 128, 32, 129] {
            // One run of 8.
            f.extend_from_slice(&[128 + 8, ch_val]);
        }
        let img = decode_radiance(&f).unwrap();
        assert_eq!(img.pixels.len(), 8);
        assert!((img.pixels[7][1] - 1.0).abs() < 1e-6);
        assert!((img.pixels[3][0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_radiance(b"hello").is_err());
        let mut f = header(4, 4);
        f.extend_from_slice(&[0; 8]);
        assert!(decode_radiance(&f).is_err());
    }

    #[test]
    fn up_samples_the_top_row_and_front_is_the_centre() {
        let img = Equirect {
            width: 4,
            height: 2,
            pixels: vec![
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        };
        assert!(img.sample(Vec3::Y).x > 0.9);
        assert!(img.sample(Vec3::NEG_Y).z > 0.9);
    }

    #[test]
    fn cubemap_has_expected_size() {
        let img = Equirect {
            width: 2,
            height: 1,
            pixels: vec![[1.0; 3]; 2],
        };
        let cube = equirect_to_cubemap(&img, 8);
        assert_eq!(cube.data.as_ref().unwrap().len(), 8 * 8 * 6 * 8);
    }
}
