//! The 24 axis-aligned orientations (SPEC.md, "Units, frames and brick dimensions").
//!
//! An orientation is stored as an index 0..=23. Index 0 is the identity. The other 23 are the
//! rotations reached by breadth-first search from the identity with the generators "turn 90
//! degrees about Z", then X, then Y, so the numbering is fixed and never depends on a platform.

use std::sync::OnceLock;

use axl_parts::types::Axis;

/// A 3x3 integer rotation matrix, row major. `w = M v` maps a part-frame vector to the robot frame.
pub type Mat = [[i32; 3]; 3];

const IDENTITY: Mat = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
/// 90 degrees counter-clockwise about +Z.
const RZ: Mat = [[0, -1, 0], [1, 0, 0], [0, 0, 1]];
/// 90 degrees counter-clockwise about +X.
const RX: Mat = [[1, 0, 0], [0, 0, -1], [0, 1, 0]];
/// 90 degrees counter-clockwise about +Y.
const RY: Mat = [[0, 0, 1], [0, 1, 0], [-1, 0, 0]];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut r = [[0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn table() -> &'static [Mat; 24] {
    static T: OnceLock<[Mat; 24]> = OnceLock::new();
    T.get_or_init(|| {
        let mut found: Vec<Mat> = vec![IDENTITY];
        let mut i = 0;
        while i < found.len() {
            let m = found[i];
            for g in [&RZ, &RX, &RY] {
                let n = mul(g, &m);
                if !found.contains(&n) {
                    found.push(n);
                }
            }
            i += 1;
        }
        let mut out = [IDENTITY; 24];
        out.copy_from_slice(&found);
        out
    })
}

/// One of the 24 axis-aligned orientations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Orient(u8);

/// Which world axis a rotation key turns about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnAxis {
    /// `R`: about Z.
    Z,
    /// `T`: about X.
    X,
    /// `Y`: about Y.
    Y,
}

impl Orient {
    /// The identity.
    pub const IDENTITY: Orient = Orient(0);

    /// From a stored index; `None` when it is not in 0..=23.
    pub fn from_index(i: u8) -> Option<Orient> {
        (i < 24).then_some(Orient(i))
    }

    /// The stored index.
    pub fn index(self) -> u8 {
        self.0
    }

    /// The rotation matrix.
    pub fn matrix(self) -> &'static Mat {
        &table()[self.0 as usize]
    }

    fn from_matrix(m: &Mat) -> Orient {
        let idx = table().iter().position(|t| t == m).unwrap_or(0);
        Orient(idx as u8)
    }

    /// Rotate an integer vector (part frame to robot frame).
    pub fn rotate(self, v: [i32; 3]) -> [i32; 3] {
        let m = self.matrix();
        [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
    }

    /// Rotate a real vector.
    pub fn rotate_f(self, v: [f64; 3]) -> [f64; 3] {
        let m = self.matrix();
        [0, 1, 2].map(|i| m[i][0] as f64 * v[0] + m[i][1] as f64 * v[1] + m[i][2] as f64 * v[2])
    }

    /// Rotate a signed axis.
    pub fn rotate_axis(self, a: Axis) -> Axis {
        let u = a.unit();
        let v = self.rotate([u[0] as i32, u[1] as i32, u[2] as i32]);
        axis_from(v)
    }

    /// The orientation that applies `self` first and then `world`.
    pub fn then(self, world: Orient) -> Orient {
        Orient::from_matrix(&mul(world.matrix(), self.matrix()))
    }

    /// The orientation after the user turns the part 90 degrees about a world axis (`R`, `T`, `Y`).
    pub fn turned(self, about: TurnAxis) -> Orient {
        let g = match about {
            TurnAxis::Z => &RZ,
            TurnAxis::X => &RX,
            TurnAxis::Y => &RY,
        };
        Orient::from_matrix(&mul(g, self.matrix()))
    }

    /// Turn a vector 90 degrees about a world axis (the same turn as [`Orient::turned`]).
    pub fn turn_vec(about: TurnAxis, v: [i32; 3]) -> [i32; 3] {
        Orient::IDENTITY.turned(about).rotate(v)
    }
}

/// The signed axis a unit integer vector points along.
pub fn axis_from(v: [i32; 3]) -> Axis {
    match v {
        [1, 0, 0] => Axis::PosX,
        [-1, 0, 0] => Axis::NegX,
        [0, 1, 0] => Axis::PosY,
        [0, -1, 0] => Axis::NegY,
        [0, 0, 1] => Axis::PosZ,
        _ => Axis::NegZ,
    }
}

/// The opposite axis.
pub fn flip(a: Axis) -> Axis {
    match a {
        Axis::PosX => Axis::NegX,
        Axis::NegX => Axis::PosX,
        Axis::PosY => Axis::NegY,
        Axis::NegY => Axis::PosY,
        Axis::PosZ => Axis::NegZ,
        Axis::NegZ => Axis::PosZ,
    }
}

/// Index 0, 1 or 2 of the axis (X, Y, Z), ignoring the sign.
pub fn axis_index(a: Axis) -> usize {
    match a {
        Axis::PosX | Axis::NegX => 0,
        Axis::PosY | Axis::NegY => 1,
        Axis::PosZ | Axis::NegZ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_24_distinct_proper_rotations() {
        let t = table();
        for (i, m) in t.iter().enumerate() {
            for other in &t[i + 1..] {
                assert_ne!(m, other);
            }
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            assert_eq!(det, 1);
        }
        assert_eq!(*Orient::IDENTITY.matrix(), IDENTITY);
    }

    #[test]
    fn four_turns_about_any_axis_return_home() {
        for i in 0..24 {
            let o = Orient::from_index(i).unwrap();
            for a in [TurnAxis::Z, TurnAxis::X, TurnAxis::Y] {
                let mut p = o;
                for _ in 0..4 {
                    p = p.turned(a);
                }
                assert_eq!(p, o);
            }
        }
    }

    #[test]
    fn turning_about_z_moves_x_to_y() {
        let o = Orient::IDENTITY.turned(TurnAxis::Z);
        assert_eq!(o.rotate([10, 0, 0]), [0, 10, 0]);
        assert_eq!(o.rotate_axis(Axis::PosX), Axis::PosY);
    }

    #[test]
    fn then_composes_in_order() {
        let a = Orient::IDENTITY.turned(TurnAxis::X);
        let b = Orient::IDENTITY.turned(TurnAxis::Z);
        let v = [3, 5, 7];
        assert_eq!(a.then(b).rotate(v), b.rotate(a.rotate(v)));
    }
}
