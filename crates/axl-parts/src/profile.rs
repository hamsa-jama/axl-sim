//! 2D outlines (millimetres) used to build extrusions: circles, stadiums, crosses, fillets.

use std::f64::consts::{PI, TAU};

/// A 2D point.
pub type P2 = [f64; 2];

/// An outer loop and its holes. Loops are normalised on use: the outer loop counter-clockwise,
/// holes clockwise, so the material is always on the left of every edge.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// Outer boundary.
    pub outer: Vec<P2>,
    /// Holes.
    pub holes: Vec<Vec<P2>>,
}

/// Signed area of a loop (positive when counter-clockwise).
pub fn signed_area(l: &[P2]) -> f64 {
    let n = l.len();
    (0..n)
        .map(|i| {
            let (a, b) = (l[i], l[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        * 0.5
}

/// Perimeter of a loop.
pub fn perimeter(l: &[P2]) -> f64 {
    let n = l.len();
    (0..n)
        .map(|i| {
            let (a, b) = (l[i], l[(i + 1) % n]);
            ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
        })
        .sum()
}

impl Profile {
    /// A profile with no holes.
    pub fn solid(outer: Vec<P2>) -> Self {
        Self { outer, holes: vec![] }
    }

    /// Add a hole.
    pub fn with_hole(mut self, hole: Vec<P2>) -> Self {
        self.holes.push(hole);
        self
    }

    /// Outer loop counter-clockwise, holes clockwise.
    pub fn normalised(&self) -> Profile {
        let fix = |l: &Vec<P2>, ccw: bool| {
            let mut v = l.clone();
            if (signed_area(&v) > 0.0) != ccw {
                v.reverse();
            }
            v
        };
        Profile {
            outer: fix(&self.outer, true),
            holes: self.holes.iter().map(|h| fix(h, false)).collect(),
        }
    }

    /// Polygon area (outer minus holes), mm2.
    pub fn area(&self) -> f64 {
        signed_area(&self.outer).abs() - self.holes.iter().map(|h| signed_area(h).abs()).sum::<f64>()
    }
}

/// A counter-clockwise circle of `n` points.
pub fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<P2> {
    (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

/// A counter-clockwise rectangle.
pub fn rect(cx: f64, cy: f64, w: f64, h: f64) -> Vec<P2> {
    let (x, y) = (w / 2.0, h / 2.0);
    vec![[cx - x, cy - y], [cx + x, cy - y], [cx + x, cy + y], [cx - x, cy + y]]
}

/// A stadium (rounded ends of radius `r`) whose end-circle centres are at (0,0) and (len,0).
pub fn stadium(len: f64, r: f64, seg: usize) -> Vec<P2> {
    let mut v = Vec::new();
    for i in 0..=seg {
        let a = -PI / 2.0 + PI * i as f64 / seg as f64;
        v.push([len + r * a.cos(), r * a.sin()]);
    }
    for i in 0..=seg {
        let a = PI / 2.0 + PI * i as f64 / seg as f64;
        v.push([r * a.cos(), r * a.sin()]);
    }
    v
}

/// A plus-shaped axle cross-section: `span` wide, with arms `arm` thick, centred on the origin.
pub fn plus(span: f64, arm: f64) -> Vec<P2> {
    let (s, a) = (span / 2.0, arm / 2.0);
    vec![
        [a, -s], [a, -a], [s, -a], [s, a], [a, a], [a, s],
        [-a, s], [-a, a], [-s, a], [-s, -a], [-a, -a], [-a, -s],
    ]
}

/// Replace every corner that turns by more than `min_turn_deg` with an arc of radius `r`
/// (`segs` segments), so the outline has no sharp corners. Works for convex and concave
/// corners. If an adjacent edge is too short for the full radius, the radius shrinks.
pub fn fillet(l: &[P2], r: f64, min_turn_deg: f64, segs: usize) -> Vec<P2> {
    let n = l.len();
    let mut out = Vec::with_capacity(n * 2);
    let dir = |a: P2, b: P2| {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        ([d[0] / len, d[1] / len], len)
    };
    for i in 0..n {
        let (p, pr, nx) = (l[i], l[(i + n - 1) % n], l[(i + 1) % n]);
        let (d1, len1) = dir(pr, p);
        let (d2, len2) = dir(p, nx);
        let cross = d1[0] * d2[1] - d1[1] * d2[0];
        let dot = d1[0] * d2[0] + d1[1] * d2[1];
        let turn = cross.atan2(dot);
        if turn.abs() < min_turn_deg.to_radians() {
            out.push(p);
            continue;
        }
        let t_full = r * (turn.abs() / 2.0).tan();
        // Each edge can give at most half its length to this corner.
        let t = t_full.min(len1 * 0.5).min(len2 * 0.5);
        let rr = t / (turn.abs() / 2.0).tan();
        let a = [p[0] - d1[0] * t, p[1] - d1[1] * t];
        // Centre is on the side the path turns towards.
        let side = if cross > 0.0 { 1.0 } else { -1.0 };
        let c = [a[0] - d1[1] * rr * side, a[1] + d1[0] * rr * side];
        let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
        for k in 0..=segs {
            let ang = a0 + turn * k as f64 / segs as f64;
            out.push([c[0] + rr * ang.cos(), c[1] + rr * ang.sin()]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas() {
        assert!((signed_area(&rect(0.0, 0.0, 4.0, 3.0)) - 12.0).abs() < 1e-12);
        assert!((plus(4.8, 1.8).len() == 12) as u8 == 1);
        let p = Profile::solid(plus(4.8, 1.8));
        assert!((p.area() - 14.04).abs() < 1e-9);
    }

    #[test]
    fn normalised_flips_orientation() {
        let mut o = rect(0.0, 0.0, 2.0, 2.0);
        o.reverse();
        let p = Profile::solid(o).with_hole(circle(0.0, 0.0, 0.3, 8)).normalised();
        assert!(signed_area(&p.outer) > 0.0 && signed_area(&p.holes[0]) < 0.0);
    }

    #[test]
    fn fillet_rounds_a_square_and_keeps_area_close() {
        let sq = rect(0.0, 0.0, 10.0, 10.0);
        let f = fillet(&sq, 1.0, 20.0, 6);
        assert_eq!(f.len(), 4 * 7);
        let a = signed_area(&f);
        // A rounded square loses (4 - pi) r^2.
        assert!((a - (100.0 - (4.0 - PI))).abs() < 0.05, "{a}");
    }

    #[test]
    fn fillet_handles_concave_corners() {
        let l = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [5.0, 5.0], [5.0, 10.0], [0.0, 10.0]];
        let f = fillet(&l, 0.5, 20.0, 4);
        assert!(signed_area(&f) > 0.0);
        assert!(f.len() > l.len());
    }

    #[test]
    fn stadium_length() {
        let s = stadium(32.0, 3.6, 12);
        let xs: Vec<f64> = s.iter().map(|p| p[0]).collect();
        let (lo, hi) = (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max));
        assert!((hi - lo - 39.2).abs() < 1e-9);
    }
}
