//! The floor mat texture the colour sensor reads (SPEC "Sample, hold and delay": the colour
//! sensor gets the floor colour by mapping the ray's hit point to the mat texture's UV
//! coordinates).

/// A rectangular texture laid on the floor. Colours are linear RGB, 0 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct MatTexture {
    /// Lower corner (x, y) of the textured area, metres.
    pub min: [f64; 2],
    /// Upper corner, metres.
    pub max: [f64; 2],
    /// Pixels across (x).
    pub width: usize,
    /// Pixels along y.
    pub height: usize,
    /// Row-major pixels, row 0 at `min[1]`.
    pub pixels: Vec<[f32; 3]>,
}

impl MatTexture {
    /// A texture of `width` x `height` pixels covering the rectangle, filled by `f(u, v)` with
    /// u and v in 0..1 at the pixel centre.
    pub fn from_fn(
        min: [f64; 2],
        max: [f64; 2],
        width: usize,
        height: usize,
        f: impl Fn(f64, f64) -> [f32; 3],
    ) -> Self {
        let mut pixels = Vec::with_capacity(width * height);
        for j in 0..height {
            for i in 0..width {
                pixels.push(f(
                    (i as f64 + 0.5) / width as f64,
                    (j as f64 + 0.5) / height as f64,
                ));
            }
        }
        Self {
            min,
            max,
            width,
            height,
            pixels,
        }
    }

    /// The colour at a world point, or `None` outside the textured area. Nearest pixel.
    pub fn sample(&self, x: f64, y: f64) -> Option<[f32; 3]> {
        if x < self.min[0] || x >= self.max[0] || y < self.min[1] || y >= self.max[1] {
            return None;
        }
        let u = (x - self.min[0]) / (self.max[0] - self.min[0]);
        let v = (y - self.min[1]) / (self.max[1] - self.min[1]);
        let i = ((u * self.width as f64) as usize).min(self.width - 1);
        let j = ((v * self.height as f64) as usize).min(self.height - 1);
        self.pixels.get(j * self.width + i).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_maps_world_points_to_pixels() {
        let t = MatTexture::from_fn([0.0, 0.0], [1.0, 0.5], 10, 5, |u, _| {
            if u < 0.5 { [0.0; 3] } else { [1.0; 3] }
        });
        assert_eq!(t.sample(0.2, 0.1), Some([0.0; 3]));
        assert_eq!(t.sample(0.7, 0.4), Some([1.0; 3]));
        assert_eq!(t.sample(1.2, 0.1), None);
        assert_eq!(t.sample(-0.01, 0.1), None);
    }
}
