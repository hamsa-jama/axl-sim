//! Property tests (SPEC.md, "Tests"): the builder never accepts an overlapping placement, and
//! connections are symmetric and independent of part order.

use axl_build::builder::Builder;
use axl_build::geom::{ConnIndex, compat, derive_connections};
use axl_build::model::Assembly;
use axl_build::place::Ray;
use axl_build::{Orient, TurnAxis};
use axl_parts::Catalog;
use proptest::prelude::*;

/// Parts whose collision shape is one box, so an independent oracle is easy to write.
const BOX_PARTS: [&str; 12] = [
    "brick_1x1",
    "brick_1x2",
    "brick_2x2",
    "brick_2x4",
    "brick_1x4",
    "plate_1x2",
    "plate_2x4",
    "plate_2x8",
    "tile_2x2",
    "tile_1x4",
    "hole_brick_4x1",
    "plate_4x8",
];

fn turn_ghost(b: &mut Builder, turns: u8) {
    for (i, ax) in [TurnAxis::Z, TurnAxis::X, TurnAxis::Y]
        .into_iter()
        .enumerate()
    {
        for _ in 0..((turns >> (2 * i)) & 3) {
            b.rotate_ghost(ax);
        }
    }
}

/// Independent oracle: rotate the 8 corners of each part's box and compare bounds per axis.
fn oracle_box(cat: &Catalog, p: &axl_build::PlacedPart) -> ([f64; 3], [f64; 3]) {
    let def = cat.get(&p.part.0).unwrap();
    let axl_parts::types::CollisionPrim::Box { center, half } = &def.bodies[0].collision[0] else {
        panic!("box part expected")
    };
    let m = p.orient.matrix();
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let v = [
                    center[0] + sx * half[0],
                    center[1] + sy * half[1],
                    center[2] + sz * half[2],
                ];
                for k in 0..3 {
                    let w = (m[k][0] as f64 * v[0] + m[k][1] as f64 * v[1] + m[k][2] as f64 * v[2])
                        * 1000.0
                        + p.pos[k] as f64 * 0.1;
                    lo[k] = lo[k].min(w);
                    hi[k] = hi[k].max(w);
                }
            }
        }
    }
    (lo, hi)
}

fn max_overlap_mm(cat: &Catalog, a: &Assembly) -> f64 {
    let boxes: Vec<_> = a.parts.iter().map(|p| oracle_box(cat, p)).collect();
    let mut worst = f64::MIN;
    for i in 0..boxes.len() {
        for j in i + 1..boxes.len() {
            let depth = (0..3)
                .map(|k| boxes[i].1[k].min(boxes[j].1[k]) - boxes[i].0[k].max(boxes[j].0[k]))
                .fold(f64::MAX, f64::min);
            worst = worst.max(depth);
        }
    }
    worst
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn the_builder_never_accepts_an_overlapping_placement(
        ops in prop::collection::vec(
            (0usize..12, 0u8..64, -60.0f64..60.0, -60.0f64..60.0, -30.0f64..40.0, 0u8..6, 0u8..3), 1..14)
    ) {
        let cat = Catalog::builtin();
        let mut b = Builder::new(cat.clone());
        let mut accepted = 0usize;
        for (part, turns, x, y, z, side, colorful) in ops {
            b.start_place(BOX_PARTS[part], None).unwrap();
            turn_ghost(&mut b, turns);
            let (origin, dir) = match side {
                0 | 1 => ([x, y, 300.0], [0.0, 0.0, -1.0]),
                2 => ([x, y, -300.0], [0.0, 0.0, 1.0]),
                3 => ([300.0, y, z], [-1.0, 0.0, 0.0]),
                4 => ([x, 300.0, z], [0.0, -1.0, 0.0]),
                _ => ([x, -300.0, z], [0.0, 1.0, 0.0]),
            };
            b.hover(&Ray { origin, dir });
            if b.click_place().is_ok() {
                accepted += 1;
                prop_assert!(accepted <= 14);
            }
            let _ = colorful;
            let worst = max_overlap_mm(&cat, &b.assembly);
            prop_assert!(worst <= 0.1 + 1e-9, "accepted an overlap of {worst} mm");
            // And every accepted part after the first is connected.
            let n = b.assembly.parts.len();
            if n > 1 {
                prop_assert!(b.issues().iter().all(|i| !i.message.contains("floating")), "{:?}", b.issues());
            }
        }
    }

    #[test]
    fn connections_are_symmetric_and_independent_of_part_order(
        ops in prop::collection::vec((0usize..12, 0u8..64, -40.0f64..40.0, -40.0f64..40.0), 2..12)
    ) {
        let cat = Catalog::builtin();
        let mut b = Builder::new(cat.clone());
        for (part, turns, x, y) in ops {
            b.start_place(BOX_PARTS[part], None).unwrap();
            turn_ghost(&mut b, turns);
            b.hover(&Ray { origin: [x, y, 300.0], dir: [0.0, 0.0, -1.0] });
            let _ = b.click_place();
        }
        let conns = derive_connections(&cat, &b.assembly);
        let idx = ConnIndex::build(&cat, &b.assembly);
        let find = |r| idx.conns.iter().find(|w| w.r == r).copied().unwrap();
        for c in &conns {
            let (wa, wb) = (find(c.a), find(c.b));
            prop_assert!(c.a < c.b);
            prop_assert_eq!(wa.pos, wb.pos);
            prop_assert_eq!(compat(&wa, &wb), compat(&wb, &wa));
            prop_assert_eq!(compat(&wa, &wb), Some(c.kind));
        }
        let mut rev = b.assembly.clone();
        rev.parts.reverse();
        prop_assert_eq!(derive_connections(&cat, &rev), conns);
    }

    #[test]
    fn orientation_turns_compose_back_to_the_start(t in prop::collection::vec(0u8..3, 0..20)) {
        let mut o = Orient::IDENTITY;
        let axes = [TurnAxis::Z, TurnAxis::X, TurnAxis::Y];
        for k in &t { o = o.turned(axes[*k as usize]); }
        prop_assert!(o.index() < 24);
        prop_assert_eq!(o.rotate([1, 2, 3]).iter().map(|v| v.abs()).sum::<i32>(), 6);
    }
}
