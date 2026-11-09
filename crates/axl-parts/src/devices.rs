//! Housings for the hub, motors, servo and sensors. Each is built from the same blocks,
//! cylinders and extrusions as the plain parts, with dark accents for ports and lenses.

use crate::block::{BlockSpec, block_mesh};
use crate::extrude::{Cap, extrude};
use crate::lathe::capped_cylinder;
use crate::profile::{Profile, circle, rect};
use crate::shapes::{AXLE_ARM, AXLE_SPAN, PART_BEVEL};
use crate::trimesh::{TriMesh, Xf};
use crate::types::{DeviceModel, Surface};

/// A mesh with its body index and surface.
pub type Piece = (usize, Surface, TriMesh);

fn block(nx: u32, ny: u32, height: f64, studs: bool, cavity: bool) -> TriMesh {
    block_mesh(&BlockSpec {
        nx,
        ny,
        height,
        studs,
        cavity,
        holes_y: vec![],
        bevel: PART_BEVEL,
    })
}

/// A rounded box centred at the origin.
fn rbox(sx: f64, sy: f64, sz: f64, bevel: f64) -> TriMesh {
    let c = Some(Cap::all(bevel.min(sx.min(sy).min(sz) * 0.4)));
    extrude(
        &Profile::solid(rect(0.0, 0.0, sx, sy)),
        -sz / 2.0,
        sz / 2.0,
        c,
        c,
    )
}

fn axle_hole() -> Vec<[f64; 2]> {
    crate::profile::fillet(
        &crate::profile::plus(AXLE_SPAN, AXLE_ARM),
        2.0 * PART_BEVEL,
        25.0,
        3,
    )
}

/// A round boss pointing along +Y from the origin plane, with a cross hole.
fn boss_y(radius: f64, len: f64) -> TriMesh {
    let p = Profile::solid(circle(0.0, 0.0, radius, 48)).with_hole(axle_hole());
    let c = Some(Cap::all(0.3));
    let mut m = extrude(&p, 0.0, len, c, c);
    m.transform(&Xf::rot_x(-90.0));
    m
}

/// A cylinder pointing along +X, starting at the YZ plane through `(x, y, z)`.
fn cyl_x(r: f64, h: f64, round: f64, x: f64, y: f64, z: f64) -> TriMesh {
    capped_cylinder(r, h, round, 48).transformed(&Xf::translate(x, y, z).after(&Xf::rot_y(90.0)))
}

/// Housing, ports and arrow of the hub: 8 x 6 studs, 22.4 mm tall.
fn hub() -> Vec<Piece> {
    let mut v = vec![(0, Surface::Body, block(8, 6, 22.4, true, true))];
    let (l, w) = (8.0 * 8.0 - 0.2, 6.0 * 8.0 - 0.2);
    let _ = l;
    let mut ports = TriMesh::new();
    for side in [-1.0, 1.0] {
        for x in [-16.0, 0.0, 16.0] {
            ports.append(&rbox(8.4, 1.0, 5.0, 0.2).transformed(&Xf::translate(
                x,
                side * (w / 2.0),
                11.2,
            )));
        }
    }
    v.push((0, Surface::Dark, ports));
    // Raised arrow on the top, in the gap between stud rows, pointing along +X.
    let arrow = vec![
        [-18.0, -0.6],
        [14.0, -0.6],
        [14.0, -1.4],
        [22.0, 0.0],
        [14.0, 1.4],
        [14.0, 0.6],
        [-18.0, 0.6],
    ];
    let c = Some(Cap::all(0.08));
    let a = extrude(&Profile::solid(arrow), 0.0, 0.3, c, c);
    v.push((
        0,
        Surface::Dark,
        a.transformed(&Xf::translate(0.0, 0.0, 22.4)),
    ));
    v
}

/// A motor: housing block with studs on top, output face at y = 0, shaft axis +Y.
fn motor(nx: u32, ny: u32, height: f64, boss_r: f64) -> Vec<Piece> {
    let len = ny as f64 * 8.0 - 0.2;
    let housing = block(nx, ny, height, true, false).transformed(&Xf::translate(
        0.0,
        -len / 2.0,
        -height / 2.0,
    ));
    vec![
        (0, Surface::Body, housing),
        (1, Surface::Dark, boss_y(boss_r, 2.5)),
    ]
}

fn servo() -> Vec<Piece> {
    let mut v = motor(3, 4, 24.0, 5.0);
    // Output horn: a disc with four small holes.
    let mut p = Profile::solid(circle(0.0, 0.0, 9.0, 64)).with_hole(axle_hole());
    for k in 0..4 {
        let a = std::f64::consts::FRAC_PI_2 * k as f64;
        p.holes.push(circle(6.8 * a.cos(), 6.8 * a.sin(), 1.1, 20));
    }
    let c = Some(Cap::all(0.3));
    let mut horn = extrude(&p, 2.5, 4.0, c, c);
    horn.transform(&Xf::rot_x(-90.0));
    v.push((1, Surface::Dark, horn));
    v
}

fn ultrasonic() -> Vec<Piece> {
    let mut v = vec![(0, Surface::Body, block(2, 6, 19.2, true, true))];
    let (mut rings, mut lens) = (TriMesh::new(), TriMesh::new());
    for y in [-12.0, 12.0] {
        rings.append(&cyl_x(8.0, 3.0, 0.4, 7.9 - 0.05, y, 9.6));
        lens.append(&cyl_x(6.2, 3.3, 0.3, 7.9 - 0.05, y, 9.6));
    }
    v.push((0, Surface::Steel, rings));
    v.push((0, Surface::Dark, lens));
    v
}

fn tof() -> Vec<Piece> {
    let mut v = vec![(0, Surface::Body, block(2, 4, 12.8, true, true))];
    let mut win = TriMesh::new();
    for y in [-8.0, 8.0] {
        win.append(&rbox(0.8, 6.0, 6.0, 0.3).transformed(&Xf::translate(7.9 + 0.2, y, 6.4)));
    }
    v.push((0, Surface::Dark, win));
    v
}

fn colour() -> Vec<Piece> {
    let mut v = vec![(0, Surface::Body, block(2, 2, 25.6, true, false))];
    let lens = capped_cylinder(3.6, 0.5, 0.2, 32)
        .transformed(&Xf::translate(0.0, 0.0, 0.0).after(&Xf::rot_x(180.0)));
    v.push((0, Surface::Dark, lens));
    v
}

fn touch() -> Vec<Piece> {
    let mut v = vec![(0, Surface::Body, block(2, 2, 9.6, true, true))];
    v.push((0, Surface::Dark, cyl_x(3.0, 5.0, 0.8, 7.9 - 0.05, 0.0, 4.8)));
    v
}

/// All pieces of a device housing.
pub fn device_pieces(model: DeviceModel) -> Vec<Piece> {
    match model {
        DeviceModel::Hub => hub(),
        DeviceModel::MotorMedium => motor(3, 5, 24.0, 6.0),
        DeviceModel::MotorLarge => motor(4, 6, 28.8, 7.5),
        DeviceModel::Servo => servo(),
        DeviceModel::Ultrasonic => ultrasonic(),
        DeviceModel::TimeOfFlight => tof(),
        DeviceModel::Color => colour(),
        DeviceModel::Touch => touch(),
    }
}

/// Housing size (x, y, z) in mm of the main block of each device, and its z offset, for collision boxes.
/// Returns (centre, half sizes) in mm, part frame.
pub fn housing_box(model: DeviceModel) -> ([f64; 3], [f64; 3]) {
    let foot = |n: u32| n as f64 * 8.0 - 0.2;
    match model {
        DeviceModel::Hub => ([0.0, 0.0, 11.2], [foot(8) / 2.0, foot(6) / 2.0, 11.2]),
        DeviceModel::MotorMedium => (
            [0.0, -foot(5) / 2.0, 0.0],
            [foot(3) / 2.0, foot(5) / 2.0, 12.0],
        ),
        DeviceModel::MotorLarge => (
            [0.0, -foot(6) / 2.0, 0.0],
            [foot(4) / 2.0, foot(6) / 2.0, 14.4],
        ),
        DeviceModel::Servo => (
            [0.0, -foot(4) / 2.0, 0.0],
            [foot(3) / 2.0, foot(4) / 2.0, 12.0],
        ),
        DeviceModel::Ultrasonic => ([0.0, 0.0, 9.6], [foot(2) / 2.0, foot(6) / 2.0, 9.6]),
        DeviceModel::TimeOfFlight => ([0.0, 0.0, 6.4], [foot(2) / 2.0, foot(4) / 2.0, 6.4]),
        DeviceModel::Color => ([0.0, 0.0, 12.8], [foot(2) / 2.0, foot(2) / 2.0, 12.8]),
        DeviceModel::Touch => ([0.0, 0.0, 4.8], [foot(2) / 2.0, foot(2) / 2.0, 4.8]),
    }
}

/// Stud grid (nx, ny) and top height (mm) of a device, and the centre of that grid (x, y).
pub fn device_studs(model: DeviceModel) -> (u32, u32, f64, [f64; 2]) {
    let foot = |n: u32| n as f64 * 8.0 - 0.2;
    match model {
        DeviceModel::Hub => (8, 6, 22.4, [0.0, 0.0]),
        DeviceModel::MotorMedium => (3, 5, 12.0, [0.0, -foot(5) / 2.0]),
        DeviceModel::MotorLarge => (4, 6, 14.4, [0.0, -foot(6) / 2.0]),
        DeviceModel::Servo => (3, 4, 12.0, [0.0, -foot(4) / 2.0]),
        DeviceModel::Ultrasonic => (2, 6, 19.2, [0.0, 0.0]),
        DeviceModel::TimeOfFlight => (2, 4, 12.8, [0.0, 0.0]),
        DeviceModel::Color => (2, 2, 25.6, [0.0, 0.0]),
        DeviceModel::Touch => (2, 2, 9.6, [0.0, 0.0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_device_builds_closed_pieces() {
        for m in [
            DeviceModel::Hub,
            DeviceModel::MotorMedium,
            DeviceModel::MotorLarge,
            DeviceModel::Servo,
            DeviceModel::Ultrasonic,
            DeviceModel::TimeOfFlight,
            DeviceModel::Color,
            DeviceModel::Touch,
        ] {
            let pieces = device_pieces(m);
            assert!(!pieces.is_empty());
            for (_, _, mesh) in &pieces {
                assert!(mesh.triangles() > 0);
                assert!(mesh.is_closed(), "{m:?}");
            }
        }
    }

    #[test]
    fn motor_output_face_is_at_the_origin() {
        let p = device_pieces(DeviceModel::MotorMedium);
        let (lo, hi) = p[0].2.bounds();
        assert!((hi[1]).abs() < 1e-9 && (lo[1] + 39.8).abs() < 1e-9);
        let (lo, hi) = p[1].2.bounds();
        assert!(lo[1].abs() < 1e-9 && (hi[1] - 2.5).abs() < 1e-9);
    }
}
