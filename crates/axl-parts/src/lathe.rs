//! Solids of revolution about Z, from a profile of (radius, z) points.

use crate::trimesh::TriMesh;

/// One point of a lathe profile.
#[derive(Clone, Copy, Debug)]
pub struct Lp {
    /// Distance from the axis, mm.
    pub r: f64,
    /// Height, mm.
    pub z: f64,
}

/// Shorthand constructor.
pub fn lp(r: f64, z: f64) -> Lp {
    Lp { r, z }
}

/// A quarter-circle fillet from `from` to `to` bulging about `centre` (points inclusive).
pub fn arc(centre: Lp, radius: f64, a0_deg: f64, a1_deg: f64, segs: usize) -> Vec<Lp> {
    (0..=segs)
        .map(|k| {
            let a = (a0_deg + (a1_deg - a0_deg) * k as f64 / segs as f64).to_radians();
            lp(centre.r + radius * a.cos(), centre.z + radius * a.sin())
        })
        .collect()
}

/// Revolve a profile. Walk the profile with the solid on the left (for a solid cylinder:
/// axis bottom, out along the bottom, up the side, in across the top). Normals are averaged
/// across profile points, except where `creases` lists a point index, which stays sharp.
pub fn lathe(profile: &[Lp], creases: &[usize], segs: usize) -> TriMesh {
    let m = profile.len();
    // Outward normal of each segment (right-hand side of travel).
    let seg_n: Vec<(f64, f64)> = (0..m - 1)
        .map(|i| {
            let (dr, dz) = (profile[i + 1].r - profile[i].r, profile[i + 1].z - profile[i].z);
            let len = (dr * dr + dz * dz).sqrt().max(1e-12);
            (dz / len, -dr / len)
        })
        .collect();
    let point_n = |i: usize, seg: usize| -> (f64, f64) {
        let p = profile[i];
        let own = seg_n[seg];
        if p.r < 1e-9 {
            return (0.0, own.1.signum());
        }
        if creases.contains(&i) || i == 0 || i == m - 1 {
            return own;
        }
        let other = seg_n[if seg + 1 == i { i } else { i - 1 }];
        let s = (own.0 + other.0, own.1 + other.1);
        let l = (s.0 * s.0 + s.1 * s.1).sqrt().max(1e-12);
        (s.0 / l, s.1 / l)
    };
    let mut mesh = TriMesh::new();
    for s in 0..m - 1 {
        let (a, b) = (profile[s], profile[s + 1]);
        let (na, nb) = (point_n(s, s), point_n(s + 1, s));
        let mut ra = vec![];
        let mut rb = vec![];
        for k in 0..=segs {
            let ph = std::f64::consts::TAU * k as f64 / segs as f64;
            let (sp, cp) = ph.sin_cos();
            ra.push(mesh.vert([a.r * cp, a.r * sp, a.z], [na.0 * cp, na.0 * sp, na.1]));
            rb.push(mesh.vert([b.r * cp, b.r * sp, b.z], [nb.0 * cp, nb.0 * sp, nb.1]));
        }
        for k in 0..segs {
            let n3 = [seg_n[s].0 * (std::f64::consts::TAU * (k as f64 + 0.5) / segs as f64).cos(),
                      seg_n[s].0 * (std::f64::consts::TAU * (k as f64 + 0.5) / segs as f64).sin(),
                      seg_n[s].1];
            if a.r < 1e-9 {
                mesh.tri_facing(ra[k], rb[k], rb[k + 1], n3);
            } else if b.r < 1e-9 {
                mesh.tri_facing(ra[k], rb[k], ra[k + 1], n3);
            } else {
                mesh.tri_facing(ra[k], rb[k], rb[k + 1], n3);
                mesh.tri_facing(ra[k], rb[k + 1], ra[k + 1], n3);
            }
        }
    }
    mesh
}

/// A cylinder of radius `r` from z = 0 to `h`, with a rounded top edge of radius `round`.
pub fn capped_cylinder(r: f64, h: f64, round: f64, segs: usize) -> TriMesh {
    let mut p = vec![lp(0.0, 0.0), lp(r, 0.0)];
    if round > 0.0 {
        p.extend(arc(lp(r - round, h - round), round, 0.0, 90.0, 3));
    } else {
        p.push(lp(r, h));
    }
    p.push(lp(0.0, h));
    lathe(&p, &[1], segs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn cylinder_volume() {
        let m = capped_cylinder(2.4, 1.7, 0.0, 64);
        let ideal = PI * 2.4 * 2.4 * 1.7;
        assert!((m.volume() - ideal).abs() / ideal < 0.003);
        assert!(m.is_closed());
    }

    #[test]
    fn rounded_stud_is_slightly_smaller() {
        let sharp = capped_cylinder(2.4, 1.7, 0.0, 48).volume();
        let round = capped_cylinder(2.4, 1.7, 0.25, 48).volume();
        assert!(round < sharp && round > 0.98 * sharp);
    }

    #[test]
    fn sphere_volume() {
        let r = 8.0;
        let p: Vec<Lp> = (0..=24)
            .map(|k| {
                let t = (-90.0 + 180.0 * k as f64 / 24.0f64).to_radians();
                lp(r * t.cos(), r * t.sin())
            })
            .collect();
        let m = lathe(&p, &[], 48);
        let ideal = 4.0 / 3.0 * PI * r.powi(3);
        assert!((m.volume() - ideal).abs() / ideal < 0.01);
        assert!(m.is_closed());
    }
}
