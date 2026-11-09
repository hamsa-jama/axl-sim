//! Editing commands: snapping, move, duplicate, recolour, history depth, devices and validation text.

use axl_build::builder::{BuildError, Builder, HISTORY_DEPTH};
use axl_build::place::Ray;
use axl_build::{Port, TurnAxis};
use axl_parts::Catalog;

fn down(x: f64, y: f64) -> Ray {
    Ray {
        origin: [x, y, 300.0],
        dir: [0.0, 0.0, -1.0],
    }
}

fn place(b: &mut Builder, part: &str, ray: Ray) -> Result<Vec<u32>, BuildError> {
    b.start_place(part, None)?;
    b.hover(&ray);
    b.click_place()
}

#[test]
fn the_first_part_rests_on_the_floor_on_the_stud_grid() {
    let mut b = Builder::new(Catalog::builtin());
    let uid = place(&mut b, "brick_2x4", down(3.0, 5.0)).unwrap()[0];
    let p = b.assembly.get(uid).unwrap();
    assert_eq!(p.pos[2], 0);
    assert_eq!(p.pos[0] % 80, 0);
    assert_eq!(p.pos[1] % 80, 0);
}

#[test]
fn a_part_snaps_within_6mm_and_not_beyond() {
    let mut b = Builder::new(Catalog::builtin());
    place(&mut b, "plate_4x8", down(0.0, 0.0)).unwrap();
    // 2 mm off the stud grid: snaps exactly onto it.
    b.start_place("brick_2x2", None).unwrap();
    b.hover(&down(2.0, 1.5));
    let g = b.ghost.as_ref().unwrap();
    assert!(g.snap.is_some() && g.valid, "{g:?}");
    assert!(
        g.anchor[0] % 40 == 0 || g.anchor[0] % 80 == 0,
        "on the 4 mm half grid: {:?}",
        g.anchor
    );
    assert_eq!(g.anchor[2], 32, "sits on the plate top");
    // Far from the plate: no snap, not valid.
    b.hover(&Ray {
        origin: [200.0, 200.0, 300.0],
        dir: [0.0, 0.0, -1.0],
    });
    let g = b.ghost.as_ref().unwrap();
    assert!(g.snap.is_none() && !g.valid);
    assert_eq!(
        b.click_place(),
        Err(BuildError::Invalid(
            "This part must connect to the robot. Move it onto a free stud, hole or axle.".into()
        ))
    );
}

#[test]
fn an_overlapping_placement_is_refused() {
    let mut b = Builder::new(Catalog::builtin());
    place(&mut b, "brick_2x4", down(0.0, 0.0)).unwrap();
    // Aim from the side so the ghost brick would sit inside the first one when forced.
    b.start_place("brick_2x4", None).unwrap();
    b.hover(&Ray {
        origin: [0.0, 0.0, 4.0],
        dir: [0.0, 0.0, -1.0],
    });
    if let Some(g) = &mut b.ghost {
        g.anchor = [0, 0, 0];
        g.snap = None;
        g.placed = true;
    }
    assert_eq!(
        b.click_place(),
        Err(BuildError::Invalid(
            "This part would overlap another part.".into()
        ))
    );
    assert_eq!(b.assembly.parts.len(), 1);
}

#[test]
fn rotate_keys_turn_the_ghost() {
    let mut b = Builder::new(Catalog::builtin());
    b.start_place("brick_1x4", None).unwrap();
    b.rotate_ghost(TurnAxis::Z);
    assert_eq!(
        b.ghost.as_ref().unwrap().items[0].orient.rotate([1, 0, 0]),
        [0, 1, 0]
    );
    b.rotate_ghost(TurnAxis::X);
    b.rotate_ghost(TurnAxis::Y);
    assert_ne!(b.ghost.as_ref().unwrap().items[0].orient.index(), 0);
}

#[test]
fn select_delete_recolour_duplicate_move_and_undo() {
    let mut b = Builder::new(Catalog::builtin());
    place(&mut b, "plate_4x8", down(0.0, 0.0)).unwrap();
    let brick = place(&mut b, "brick_2x2", down(0.0, 0.0)).unwrap()[0];
    // Recolour.
    b.select(Some(brick), false);
    assert_eq!(b.recolor_selected("blue"), Ok(1));
    assert_eq!(b.assembly.get(brick).unwrap().color.0, "blue");
    assert!(b.recolor_selected("no_such_colour").is_err());
    // Duplicate: the copy is placed next to the original.
    b.start_duplicate().unwrap();
    b.hover(&down(16.0, 0.0));
    let copy = b.click_place().unwrap()[0];
    assert_ne!(copy, brick);
    assert_eq!(b.assembly.parts.len(), 3);
    assert_eq!(b.assembly.get(copy).unwrap().color.0, "blue");
    // Move the copy: uid is kept, one history step.
    let steps = b.history_len().0;
    b.select(Some(copy), false);
    b.start_move().unwrap();
    assert_eq!(b.assembly.parts.len(), 2, "picked up");
    b.hover(&down(-16.0, 0.0));
    assert_eq!(b.click_place().unwrap(), vec![copy]);
    assert_eq!(b.history_len().0, steps + 1);
    let moved_to = b.assembly.get(copy).unwrap().pos;
    b.undo();
    assert_ne!(b.assembly.get(copy).unwrap().pos, moved_to);
    // A cancelled move puts the part back.
    b.select(Some(copy), false);
    b.start_move().unwrap();
    b.cancel();
    assert!(b.assembly.get(copy).is_some());
    // Additive selection and delete.
    b.select(Some(brick), false);
    b.select(Some(copy), true);
    assert_eq!(b.selection.len(), 2);
    assert_eq!(b.delete_selected(), Ok(2));
    assert_eq!(b.assembly.parts.len(), 1);
    // uids are never reused, even after undo and a new placement.
    b.undo();
    let fresh = place(&mut b, "brick_1x1", down(-8.0, 8.0)).unwrap()[0];
    assert!(fresh > copy);
}

#[test]
fn history_keeps_200_steps() {
    let mut b = Builder::new(Catalog::builtin());
    place(&mut b, "plate_8x16", down(0.0, 0.0)).unwrap();
    for i in 0..(HISTORY_DEPTH + 20) {
        b.set_name(&format!("robot{i}"));
    }
    assert_eq!(b.history_len().0, HISTORY_DEPTH);
    let mut n = 0;
    while b.undo().is_some() {
        n += 1;
    }
    assert_eq!(n, HISTORY_DEPTH);
}

#[test]
fn devices_get_names_and_the_next_free_port_and_validation_reports_clashes() {
    let mut b = Builder::new(Catalog::builtin());
    place(&mut b, "plate_8x16", down(0.0, 0.0)).unwrap();
    // Seven sensors hung from the plate (only the first six free ports exist).
    let mut uids = vec![];
    for i in 0..7 {
        let x = -28.0 + 8.0 * i as f64;
        uids.push(
            place(
                &mut b,
                "sensor_touch",
                Ray {
                    origin: [x, -40.0, -300.0],
                    dir: [0.0, 0.0, 1.0],
                },
            )
            .unwrap()[0],
        );
    }
    let ports: Vec<_> = uids
        .iter()
        .map(|u| b.assembly.get(*u).unwrap().device.as_ref().unwrap().port)
        .collect();
    assert_eq!(&ports[..3], &[Some(Port::A), Some(Port::B), Some(Port::C)]);
    let names: Vec<_> = uids
        .iter()
        .map(|u| {
            b.assembly
                .get(*u)
                .unwrap()
                .device
                .as_ref()
                .unwrap()
                .name
                .clone()
        })
        .collect();
    assert_eq!(names[0], "touch");
    assert_eq!(names[1], "touch_2");
    let msgs: Vec<_> = b.issues().iter().map(|i| i.message.clone()).collect();
    assert!(msgs.contains(&"A robot needs exactly one hub.".to_string()));
    assert!(msgs.contains(&"The hub has six ports. Remove a motor or sensor.".to_string()));
    b.set_device_name(uids[1], "touch").unwrap();
    assert!(
        b.issues()
            .iter()
            .any(|i| i.message == "Two devices are called touch.")
    );
    b.set_device_port(uids[1], Port::A).unwrap();
    assert!(
        b.issues()
            .iter()
            .any(|i| i.message == "Two devices use port A.")
    );
    assert_eq!(b.set_device_name(uids[0], "9bad"), Err(BuildError::BadName));
}
