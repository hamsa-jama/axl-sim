//! Generators for gears, beams, axles, pins, wheels and the caster, with analytic volumes.

use std::f64::consts::{PI, TAU};

use crate::block::{BlockSpec, block_mesh};
use crate::extrude::{Cap, bevel_loss, extrude};
use crate::lathe::{Lp, arc, lathe, lp};
use crate::profile::{P2, Profile, circle, fillet, perimeter, plus, rect, signed_area, stadium};
use crate::trimesh::{TriMesh, Xf};
use crate::types::BeamShape;

/// Hole spacing and unit length, mm.
pub const UNIT: f64 = 8.0;
/// Beam cross-section, mm.
pub const BEAM_W: f64 = 7.2;
/// Beam edge bevel, mm.
pub const BEAM_BEVEL: f64 = 0.3;
/// Brick, plate and gear edge bevel, mm.
pub const PART_BEVEL: f64 = 0.15;
/// Round hole diameter, mm.
pub const HOLE_D: f64 = 4.8;
/// Axle cross-section span and arm width, mm.
pub const AXLE_SPAN: f64 = 4.8;
/// Axle arm width, mm.
pub const AXLE_ARM: f64 = 1.8;
/// Pin and axle round radius, mm.
pub const PIN_R: f64 = 2.4;
const SEG: usize = 32;

/// The ideal area of the axle cross (with the corner rounding the mesh uses), mm2.
pub fn axle_area() -> f64 {
    let r = 2.0 * PART_BEVEL;
    AXLE_SPAN * AXLE_ARM * 2.0 - AXLE_ARM * AXLE_ARM - 4.0 * (1.0 - PI / 4.0) * r * r
}

fn axle_profile() -> Vec<P2> {
    fillet(&plus(AXLE_SPAN, AXLE_ARM), 2.0 * PART_BEVEL, 25.0, 3)
}

// ---------------------------------------------------------------- gears

fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// Outline of a module-1 spur gear with 20 degree pressure angle (counter-clockwise, mm).
pub fn gear_outline(teeth: u32) -> Vec<P2> {
    let n = teeth as f64;
    let rp = n / 2.0;
    let rb = rp * 20f64.to_radians().cos();
    let ra = rp + 1.0;
    let rf = rp - 1.25;
    let inv_p = inv(20f64.to_radians());
    let psi = PI / (2.0 * n);
    let half_root = psi + inv_p;
    let flank = |r: f64| -> f64 {
        // Polar angle of the left flank (positive side) at radius r.
        if r <= rb { half_root } else { half_root - inv((rb / r).acos()) }
    };
    let mut radii = vec![rf];
    if rb > rf {
        radii.push(rb);
    }
    let from = rb.max(rf);
    let steps = 6;
    for k in 1..=steps {
        radii.push(from + (ra - from) * k as f64 / steps as f64);
    }
    let mut out = Vec::new();
    for t in 0..teeth {
        let c = TAU * t as f64 / n;
        let pt = |r: f64, a: f64| [r * (c + a).cos(), r * (c + a).sin()];
        // Right flank, root to tip.
        for &r in &radii {
            out.push(pt(r, -flank(r)));
        }
        // Tip land.
        let tip = flank(ra);
        for k in 1..=2 {
            out.push(pt(ra, -tip + 2.0 * tip * k as f64 / 3.0));
        }
        // Left flank, tip to root.
        for &r in radii.iter().rev() {
            out.push(pt(r, flank(r)));
        }
        // Root land to the next tooth.
        let gap = TAU / n - 2.0 * half_root;
        for k in 1..=2 {
            out.push(pt(rf, half_root + gap * k as f64 / 3.0));
        }
    }
    out
}

/// Edge bevel of a gear: 0.15 mm, less on gears whose root circle leaves only a thin wall
/// around the axle cross (the 8-tooth gear).
pub fn gear_bevel(teeth: u32) -> f64 {
    let wall = teeth as f64 / 2.0 - 1.25 - 2.56;
    PART_BEVEL.min(0.2 * wall.max(0.0))
}

/// Gear mesh: axis along Z, centred on the origin.
pub fn gear_mesh(teeth: u32, thickness: f64) -> TriMesh {
    let outer = fillet(&gear_outline(teeth), 0.25, 25.0, 3);
    let p = Profile::solid(outer).with_hole(axle_profile());
    let b = Cap::all(gear_bevel(teeth));
    extrude(&p, -thickness / 2.0, thickness / 2.0, Some(b), Some(b))
}

/// Analytic gear volume, mm3: tooth outline area (shoelace over the exact involute
/// polygon) minus the axle cross, times thickness, minus the bevels.
pub fn gear_volume(teeth: u32, thickness: f64) -> f64 {
    let o = gear_outline(teeth);
    let area = signed_area(&o).abs() - axle_area();
    area * thickness - 2.0 * bevel_loss(perimeter(&o) + perimeter(&axle_profile()), gear_bevel(teeth))
}

// ---------------------------------------------------------------- beams

fn holes_at(pts: &[(f64, f64)]) -> Vec<Vec<P2>> {
    pts.iter().map(|&(x, y)| circle(x, y, HOLE_D / 2.0, SEG)).collect()
}

/// Hole centres of a beam, in the beam's XY outline plane (X along the length).
pub fn beam_hole_centres(shape: BeamShape) -> Vec<(f64, f64)> {
    match shape {
        BeamShape::Straight { holes } => (0..holes).map(|i| (i as f64 * UNIT, 0.0)).collect(),
        BeamShape::L { long, short } => {
            let mut v: Vec<(f64, f64)> = (0..long).map(|i| (i as f64 * UNIT, 0.0)).collect();
            let x = (long - 1) as f64 * UNIT;
            v.extend((1..short).map(|j| (x, j as f64 * UNIT)));
            v
        }
        BeamShape::Frame { w, h } => {
            let mut v = Vec::new();
            for i in 0..w {
                for j in 0..h {
                    if i == 0 || j == 0 || i == w - 1 || j == h - 1 {
                        v.push((i as f64 * UNIT, j as f64 * UNIT));
                    }
                }
            }
            v
        }
    }
}

fn beam_outline(shape: BeamShape) -> (Vec<P2>, Vec<Vec<P2>>) {
    let r = BEAM_W / 2.0;
    match shape {
        BeamShape::Straight { holes } => (stadium((holes - 1) as f64 * UNIT, r, 16), vec![]),
        BeamShape::L { long, short } => {
            let x = (long - 1) as f64 * UNIT;
            let y = (short - 1) as f64 * UNIT;
            let mut o = Vec::new();
            let arcp = |o: &mut Vec<P2>, cx: f64, cy: f64, a0: f64, a1: f64| {
                for k in 0..=12 {
                    let a = (a0 + (a1 - a0) * k as f64 / 12.0).to_radians();
                    o.push([cx + r * a.cos(), cy + r * a.sin()]);
                }
            };
            o.push([0.0, -r]);
            arcp(&mut o, x, 0.0, -90.0, 0.0);
            arcp(&mut o, x, y, 0.0, 180.0);
            o.push([x - r, r]);
            arcp(&mut o, 0.0, 0.0, 90.0, 270.0);
            o.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
            if let (Some(f), Some(l)) = (o.first().copied(), o.last().copied())
                && (f[0] - l[0]).abs() < 1e-9
                && (f[1] - l[1]).abs() < 1e-9
            {
                o.pop();
            }
            (fillet(&o, 1.0, 40.0, 4), vec![])
        }
        BeamShape::Frame { w, h } => {
            let (lx, ly) = ((w - 1) as f64 * UNIT, (h - 1) as f64 * UNIT);
            let outer = fillet(&rect(lx / 2.0, ly / 2.0, lx + BEAM_W, ly + BEAM_W), r, 20.0, 12);
            let inner = fillet(&rect(lx / 2.0, ly / 2.0, lx - BEAM_W, ly - BEAM_W), 1.0, 20.0, 4);
            (outer, vec![inner])
        }
    }
}

/// Beam mesh. Part frame: origin at the centre of the first hole, holes along +X, hole axes along Y.
pub fn beam_mesh(shape: BeamShape) -> TriMesh {
    let (outer, extra) = beam_outline(shape);
    let mut p = Profile::solid(outer);
    p.holes = extra;
    p.holes.extend(holes_at(&beam_hole_centres(shape)));
    let b = Cap::all(BEAM_BEVEL);
    let mut m = extrude(&p, -BEAM_W / 2.0, BEAM_W / 2.0, Some(b), Some(b));
    // Profile Y becomes part Z, extrusion Z becomes part -Y.
    m.transform(&Xf::rot_x(90.0));
    m
}

/// Analytic beam volume, mm3: ideal outline area (true circles) minus the holes, times
/// the thickness, minus the bevels.
pub fn beam_volume(shape: BeamShape) -> f64 {
    let r = BEAM_W / 2.0;
    let hole_a = PI * (HOLE_D / 2.0).powi(2);
    let n_holes = beam_hole_centres(shape).len() as f64;
    let area = match shape {
        BeamShape::Straight { holes } => (holes - 1) as f64 * UNIT * BEAM_W + PI * r * r - n_holes * hole_a,
        BeamShape::L { long, short } => {
            let (x, y) = ((long - 1) as f64 * UNIT, (short - 1) as f64 * UNIT);
            // Two arms, minus their overlap, plus two end half-discs and the rounded outer corner;
            // the inner corner fillet (r = 1) adds a sliver.
            BEAM_W * (x + y) - r * r + PI * r * r * 1.25 + (1.0 - PI / 4.0) - n_holes * hole_a
        }
        BeamShape::Frame { w, h } => {
            let (lx, ly) = ((w - 1) as f64 * UNIT, (h - 1) as f64 * UNIT);
            let outer = (lx + BEAM_W) * (ly + BEAM_W) - (4.0 - PI) * r * r;
            let inner = (lx - BEAM_W) * (ly - BEAM_W) - (4.0 - PI);
            outer - inner - n_holes * hole_a
        }
    };
    let (o, extra) = beam_outline(shape);
    let per = perimeter(&o)
        + extra.iter().map(|l| perimeter(l)).sum::<f64>()
        + n_holes * PI * HOLE_D;
    area * BEAM_W - 2.0 * bevel_loss(per, BEAM_BEVEL)
}

// ---------------------------------------------------------------- axles and pins

/// Axle mesh along +X: the first 8 mm slot is centred on the origin, so it spans
/// x = -4 to 8 n - 4.
pub fn axle_mesh(units: u32) -> TriMesh {
    let p = Profile::solid(axle_profile());
    let b = Cap::all(PART_BEVEL);
    let mut m = extrude(&p, -UNIT / 2.0, UNIT * units as f64 - UNIT / 2.0, Some(b), Some(b));
    // Extrusion Z becomes X (x = z).
    m.transform(&Xf::rot_y(90.0));
    m
}

/// Analytic axle volume, mm3.
pub fn axle_volume(units: u32) -> f64 {
    axle_area() * UNIT * units as f64 - 2.0 * bevel_loss(perimeter(&axle_profile()), PART_BEVEL)
}

/// A round pin along +X from x0 to x1 (mm), with chamfered ends and optional friction ridges at each slot.
fn round_pin(x0: f64, x1: f64, ridges: bool) -> TriMesh {
    let ch = 0.4;
    let mut p: Vec<Lp> = vec![lp(0.0, x0), lp(PIN_R - ch, x0), lp(PIN_R, x0 + ch)];
    if ridges {
        let slots = ((x1 - x0) / UNIT).round() as i32;
        for s in 0..slots {
            let c = x0 + UNIT * (s as f64 + 0.5);
            p.push(lp(PIN_R, c - 0.9));
            p.push(lp(PIN_R + 0.1, c - 0.5));
            p.push(lp(PIN_R + 0.1, c + 0.5));
            p.push(lp(PIN_R, c + 0.9));
        }
    }
    p.push(lp(PIN_R, x1 - ch));
    p.push(lp(PIN_R - ch, x1));
    p.push(lp(0.0, x1));
    let creases: Vec<usize> = (1..p.len() - 1).collect();
    let mut m = lathe(&p, &creases, SEG);
    // Lathe axis Z becomes X.
    m.transform(&Xf::rot_y(90.0));
    m
}

/// Pin mesh along +X, slot 1 centred on the origin.
pub fn pin_mesh(units: u32, kind: crate::types::PinKind) -> TriMesh {
    use crate::types::PinKind::*;
    let (x0, x1) = (-UNIT / 2.0, UNIT * units as f64 - UNIT / 2.0);
    match kind {
        Friction => round_pin(x0, x1, true),
        Smooth => round_pin(x0, x1, false),
        AxlePin => {
            let mid = x0 + UNIT * (units as f64 / 2.0).floor();
            let mut m = round_pin(x0, mid, false);
            let p = Profile::solid(axle_profile());
            let b = Cap::all(PART_BEVEL);
            let mut ax = extrude(&p, mid, x1, Some(b), Some(b));
            ax.transform(&Xf::rot_y(90.0));
            m.append(&ax);
            m
        }
    }
}

/// Analytic pin volume, mm3 (ends chamfered 0.4 mm, ridges 0.1 mm proud over 1 mm).
pub fn pin_volume(units: u32, kind: crate::types::PinKind) -> f64 {
    use crate::types::PinKind::*;
    let l = UNIT * units as f64;
    let cyl = |len: f64| PI * PIN_R * PIN_R * len;
    // A 0.4 mm x 45 degree chamfer removes a ring of triangular section.
    let chamfer = |ends: f64| ends * 2.0 * PI * (PIN_R - 0.4 / 3.0) * 0.08;
    let ridge = |slots: f64| slots * PI * ((PIN_R + 0.1).powi(2) - PIN_R * PIN_R) * 1.0 * 0.9;
    match kind {
        Friction => cyl(l) - chamfer(2.0) + ridge(units as f64),
        Smooth => cyl(l) - chamfer(2.0),
        AxlePin => {
            let round = UNIT * (units as f64 / 2.0).floor();
            cyl(round) - chamfer(1.0) + axle_volume_len(l - round)
        }
    }
}

fn axle_volume_len(len: f64) -> f64 {
    axle_area() * len - 2.0 * bevel_loss(perimeter(&axle_profile()), PART_BEVEL) * 0.5
}

// ---------------------------------------------------------------- wheels and caster

/// Tyre and rim of a wheel, axis along Z, centred on the origin. Returns (rim, tyre).
pub fn wheel_meshes(diameter: f64, width: f64) -> (TriMesh, TriMesh) {
    let r = diameter / 2.0;
    let tyre_t = (r * 0.18).clamp(2.5, 7.0);
    let r_in = r - tyre_t;
    // Tread: transverse grooves, about one block per 5 mm of circumference.
    let n = ((TAU * r / 5.0).round() as usize).max(8);
    let depth = 0.9;
    let mut tread: Vec<P2> = Vec::new();
    let mut put = |a: f64, rad: f64| tread.push([rad * a.cos(), rad * a.sin()]);
    for i in 0..n {
        let d = TAU / n as f64;
        let a = i as f64 * d;
        put(a, r);
        put(a + 0.50 * d, r);
        put(a + 0.60 * d, r - depth);
        put(a + 0.90 * d, r - depth);
    }
    let tread = fillet(&tread, 0.3, 30.0, 2);
    let tp = Profile::solid(tread).with_hole(circle(0.0, 0.0, r_in, 96));
    let mut tyre = extrude(&tp, -width / 2.0, width / 2.0, Some(Cap { outer: 0.6, holes: 0.2 }), Some(Cap { outer: 0.6, holes: 0.2 }));
    tyre.transform(&Xf::IDENTITY);
    // Rim: ring, web with lightening holes, hub boss.
    let b = Cap::all(PART_BEVEL * 2.0);
    let ring_w = width - 1.0;
    let ring = Profile::solid(circle(0.0, 0.0, r_in + 0.2, 96)).with_hole(circle(0.0, 0.0, r_in - 2.0, 96));
    let mut rim = extrude(&ring, -ring_w / 2.0, ring_w / 2.0, Some(b), Some(b));
    let boss_r = (r * 0.26).clamp(4.2, 6.5);
    let mut web = Profile::solid(circle(0.0, 0.0, r_in - 1.8, 96)).with_hole(axle_profile());
    if r > 20.0 {
        let holes = if r < 25.0 { 4 } else if r < 32.0 { 5 } else { 6 };
        let hr = (r_in - 2.0 - boss_r) * 0.32;
        let at = boss_r + (r_in - 2.0 - boss_r) * 0.5;
        for k in 0..holes {
            let a = TAU * (k as f64 + 0.5) / holes as f64;
            web.holes.push(circle(at * a.cos(), at * a.sin(), hr, 32));
        }
    }
    rim.append(&extrude(&web, -1.0, 1.0, Some(b), Some(b)));
    let boss = Profile::solid(circle(0.0, 0.0, boss_r, 48)).with_hole(axle_profile());
    rim.append(&extrude(&boss, -ring_w / 2.0 - 0.6, ring_w / 2.0 + 0.6, Some(b), Some(b)));
    (rim, tyre)
}

/// Caster: a 2x2 stud housing holding a steel ball. Origin at the lowest point of the ball.
/// Returns (housing, ball). Housing bottom is at z = ball radius.
pub fn caster_meshes(ball_d: f64) -> (TriMesh, TriMesh) {
    let r = ball_d / 2.0;
    let spec = BlockSpec { nx: 2, ny: 2, height: 12.8, studs: true, cavity: false, holes_y: vec![], bevel: PART_BEVEL };
    let mut housing = block_mesh(&spec);
    housing.transform(&Xf::translate(0.0, 0.0, r));
    let prof: Vec<Lp> = arc(lp(0.0, r), r, -90.0, 90.0, 24);
    let mut ball = lathe(&prof, &[], 48);
    ball.transform(&Xf::translate(0.0, 0.0, 0.0));
    (housing, ball)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PinKind;

    fn within(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() / b.abs() < tol
    }

    #[test]
    fn gear_outline_has_module_one_dimensions() {
        for n in [8u32, 12, 16, 20, 24, 36, 40] {
            let o = gear_outline(n);
            let rmax = o.iter().map(|p| p[0].hypot(p[1])).fold(0.0, f64::max);
            let rmin = o.iter().map(|p| p[0].hypot(p[1])).fold(f64::MAX, f64::min);
            assert!((rmax - (n as f64 / 2.0 + 1.0)).abs() < 1e-9, "tip radius {n}");
            assert!((rmin - (n as f64 / 2.0 - 1.25)).abs() < 1e-9, "root radius {n}");
            assert!(signed_area(&o) > 0.0);
        }
    }

    #[test]
    fn gears_are_closed_and_match_their_analytic_volume() {
        for n in [8u32, 24, 40] {
            let m = gear_mesh(n, 3.2);
            assert!(m.is_closed(), "gear {n}");
            let a = gear_volume(n, 3.2);
            assert!(within(m.volume(), a, 0.01), "gear {n}: mesh {} analytic {a}", m.volume());
        }
    }

    #[test]
    fn beams_are_closed_and_match_their_analytic_volume() {
        for s in [
            BeamShape::Straight { holes: 3 },
            BeamShape::Straight { holes: 15 },
            BeamShape::L { long: 5, short: 3 },
            BeamShape::Frame { w: 5, h: 7 },
        ] {
            let m = beam_mesh(s);
            assert!(m.is_closed(), "{s:?}");
            let a = beam_volume(s);
            assert!(within(m.volume(), a, 0.01), "{s:?}: mesh {} analytic {a}", m.volume());
        }
    }

    #[test]
    fn beam_frame_is_along_x_with_holes_along_y() {
        let m = beam_mesh(BeamShape::Straight { holes: 7 });
        let (lo, hi) = m.bounds();
        assert!((lo[0] + 3.6).abs() < 1e-6 && (hi[0] - 51.6).abs() < 1e-6);
        assert!((hi[1] - 3.6).abs() < 1e-6 && (lo[1] + 3.6).abs() < 1e-6);
        assert!((hi[2] - 3.6).abs() < 1e-6 && (lo[2] + 3.6).abs() < 1e-6);
    }

    #[test]
    fn axles_and_pins() {
        for u in [2u32, 8, 12] {
            let m = axle_mesh(u);
            assert!(m.is_closed());
            assert!(within(m.volume(), axle_volume(u), 0.01));
            let (lo, hi) = m.bounds();
            assert!((lo[0] + 4.0).abs() < 1e-6 && (hi[0] - (8.0 * u as f64 - 4.0)).abs() < 1e-6);
        }
        for (u, k) in [(2, PinKind::Friction), (2, PinKind::Smooth), (3, PinKind::Friction), (2, PinKind::AxlePin)] {
            let m = pin_mesh(u, k);
            assert!(m.is_closed(), "{k:?}");
            assert!(within(m.volume(), pin_volume(u, k), 0.02), "{k:?}: {} vs {}", m.volume(), pin_volume(u, k));
        }
    }

    #[test]
    fn wheels_are_closed_and_the_right_size() {
        for (d, w) in [(30.0, 8.0), (43.0, 12.0), (56.0, 14.0), (81.0, 18.0)] {
            let (rim, tyre) = wheel_meshes(d, w);
            assert!(tyre.is_closed(), "tyre {d}");
            assert!(rim.is_closed(), "rim {d}");
            let (lo, hi) = tyre.bounds();
            assert!(((hi[0] - lo[0]) - d).abs() < 0.2, "{d}");
            assert!((hi[2] - lo[2] - w).abs() < 1e-6);
        }
    }

    #[test]
    fn caster_ball_is_16mm() {
        let (h, b) = caster_meshes(16.0);
        let (lo, hi) = b.bounds();
        assert!((hi[2] - lo[2] - 16.0).abs() < 1e-9 && lo[2].abs() < 1e-9);
        assert!(h.is_closed());
    }
}