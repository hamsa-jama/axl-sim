//! Integration tests: the 75 shipped part files, their meshes, masses and sample volumes.

use std::path::PathBuf;

use axl_parts::generate::{ABS_DENSITY, builtin_parts};
use axl_parts::types::MassSource;
use axl_parts::{Catalog, PartDef};

fn parts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/parts")
}

fn load() -> Catalog {
    Catalog::load_dir(&parts_dir()).expect("assets/parts loads")
}

#[test]
fn all_75_part_files_load() {
    let c = load();
    assert_eq!(c.len(), 75);
}

#[test]
fn files_on_disk_match_the_generator() {
    // If this fails, run `cargo xtask gen-parts` and commit the result.
    let c = load();
    for p in builtin_parts() {
        let on_disk = c
            .get(&p.id.0)
            .unwrap_or_else(|| panic!("missing file for {}", p.id.0));
        assert_eq!(on_disk, &p, "{} differs from the generator", p.id.0);
    }
}

#[test]
fn every_part_builds_a_closed_mesh_with_sane_size() {
    for p in load().iter() {
        let pm = p.build_mesh();
        assert!(!pm.slots.is_empty(), "{}", p.id.0);
        let (lo, hi) = pm.bounds();
        for k in 0..3 {
            assert!(lo[k].is_finite() && hi[k].is_finite());
            // Nothing bigger than the 8x16 plate (128 mm) or 15-hole beam (127 mm), nothing empty.
            assert!(
                hi[k] - lo[k] < 0.14,
                "{} is {} m long on axis {k}",
                p.id.0,
                hi[k] - lo[k]
            );
        }
        assert!(pm.triangles() > 12, "{}", p.id.0);
        for s in &pm.slots {
            assert!(s.body < p.bodies.len());
            for n in &s.mesh.normals {
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!((l - 1.0).abs() < 1e-3, "{} has a non-unit normal", p.id.0);
            }
            assert!(
                s.mesh
                    .indices
                    .iter()
                    .all(|&i| (i as usize) < s.mesh.positions.len())
            );
        }
    }
}

#[test]
fn computed_masses_equal_volume_times_abs_density() {
    for p in load().iter() {
        if let Some(v) = p.solid_volume_m3() {
            let b = &p.bodies[0];
            assert!(
                matches!(b.mass_source, MassSource::Computed { .. }),
                "{}",
                p.id.0
            );
            let want = v * ABS_DENSITY;
            assert!(
                (b.mass_kg - want).abs() / want < 1e-6,
                "{}: {} vs {want}",
                p.id.0,
                b.mass_kg
            );
        } else {
            assert!(
                matches!(
                    p.bodies[0].mass_source,
                    MassSource::Placeholder | MassSource::Measured { .. }
                ),
                "{}",
                p.id.0
            );
        }
    }
}

#[test]
fn motors_use_the_spec_masses_split_between_housing_and_rotor() {
    let c = load();
    for (id, total) in [
        ("motor_medium", 0.035),
        ("motor_large", 0.080),
        ("servo_180", 0.040),
    ] {
        let p = c.get(id).unwrap();
        assert_eq!(p.bodies.len(), 2);
        let sum: f64 = p.bodies.iter().map(|b| b.mass_kg).sum();
        assert!((sum - total).abs() < 1e-6, "{id}");
        assert!(p.internal_joint.is_some());
    }
}

/// Mesh volume of the first (or only) body in mm3.
fn mesh_volume_mm3(p: &PartDef) -> f64 {
    let pm = p.build_mesh();
    // The finished mesh is in metres; sum signed tetrahedra.
    let mut v = 0.0f64;
    for s in pm
        .slots
        .iter()
        .filter(|s| s.body == 0 && s.surface == axl_parts::types::Surface::Body)
    {
        for t in s.mesh.indices.chunks(3) {
            let a = s.mesh.positions[t[0] as usize].map(|x| x as f64 * 1e3);
            let b = s.mesh.positions[t[1] as usize].map(|x| x as f64 * 1e3);
            let c = s.mesh.positions[t[2] as usize].map(|x| x as f64 * 1e3);
            v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
        }
    }
    v / 6.0
}

fn check(id: &str, hand_mm3: f64) {
    let c = load();
    let p = c.get(id).unwrap();
    let analytic = p.solid_volume_m3().unwrap() * 1e9;
    let mesh = mesh_volume_mm3(p);
    assert!(
        (analytic - hand_mm3).abs() / hand_mm3 < 0.01,
        "{id}: analytic {analytic} vs hand {hand_mm3}"
    );
    assert!(
        (mesh - hand_mm3).abs() / hand_mm3 < 0.01,
        "{id}: mesh {mesh} vs hand {hand_mm3}"
    );
}

// Sample 1: brick 2x4 (31.8 x 15.8 x 9.6 mm, walls 1.2 mm, top 1.0 mm, 8 studs, 3 tubes).
//   outer box        31.8 * 15.8 * 9.6                = 4823.4
//   hollow           (31.8-2.4) * (15.8-2.4) * 8.6    = 3388.1   (cavity depth 9.6 - 1.0)
//   3 tubes          3 * pi * (3.25^2 - 2.4^2) / ... * 8.6 = 3 * 15.0875 * 8.6 = 389.3
//   8 studs          8 * pi * 2.4^2 * 1.7             =  246.1
//   total            4823.4 - 3388.1 + 389.3 + 246.1  = 2070.7 mm3   (2.15 g of ABS)
#[test]
fn sample_brick_2x4() {
    check("brick_2x4", 2070.7);
}

// Sample 2: plate 2x8 (15.8 x 63.8 x 3.2 mm, cavity depth 2.2 mm, 7 tubes, 16 studs).
//   outer box        15.8 * 63.8 * 3.2                = 3225.7
//   hollow           13.4 * 61.4 * 2.2                = 1810.1
//   7 tubes          7 * 15.0875 * 2.2                =  232.3
//   16 studs         16 * 30.762                      =  492.2
//   total            3225.7 - 1810.1 + 232.3 + 492.2  = 2140.1 mm3
#[test]
fn sample_plate_2x8() {
    check("plate_2x8", 2140.1);
}

// Sample 3: 5-hole beam. Outline = rectangle 32 x 7.2 plus two half discs of radius 3.6
//   = 230.4 + pi * 3.6^2 = 271.12 mm2; minus 5 holes of radius 2.4: 5 * 18.096 = 90.48.
//   Area 180.64 mm2, thickness 7.2 mm: 1300.6 mm3 (the 0.3 mm bevels remove another 0.5 %).
#[test]
fn sample_beam_5() {
    check("beam_5", 1300.6);
}

// Sample 4: axle of 8 units = 64 mm. Cross section = two 4.8 x 1.8 bars minus the
//   shared 1.8 x 1.8 square = 8.64 + 8.64 - 3.24 = 14.04 mm2; times 64 mm = 898.6 mm3.
#[test]
fn sample_axle_8() {
    check("axle_8", 898.6);
}

// Sample 5: 24-tooth gear, module 1 mm, 3.2 mm thick. Pitch radius 12, tip radius 13, root radius
//   10.75, base radius 12 cos 20 = 11.276, 20 degree pressure angle. A tooth is wide at the root and
//   narrow at the tip; its width at radius r is w(r) = 2 r theta(r) with theta(r) = psi + inv(20) - inv(a(r)),
//   psi = pi / 48 = 0.06545, inv(x) = tan x - x, a(r) = acos(11.276 / r) (flanks are radial below the
//   base circle, theta = 0.080354 there). Widths: r 10.75: 1.7276; 11.276: 1.812; 11.638: 1.7497;
//   12: 1.5708; 12.5: 1.2012; 13: 0.7059 mm. Area of one tooth above the root circle, by
//   trapezoid on 10.75..11.276 (0.931) and Simpson on 11.276..12 (1.2528) and 12..13 (1.1803):
//   3.364 mm2. Root disc pi * 10.75^2 = 363.05; 24 teeth add 80.74; minus the 14.04 mm2 axle cross:
//   429.75 mm2, times 3.2 mm = 1375.2 mm3.
#[test]
fn sample_gear_24() {
    check("gear_24", 1375.2);
}
