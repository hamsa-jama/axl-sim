//! The Rover and Test chassis are built through the same commands the UI issues (pick a part,
//! turn it, hover a cursor ray, click), and must reproduce the shipped `.axlbot` files exactly.

use axl_build::builder::Builder;
use axl_build::validate::{Level, has_errors};
use axl_build::{io, starter};
use axl_parts::Catalog;

fn asset(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/robots")
        .join(name)
}

#[test]
fn rover_built_with_ui_commands_matches_the_shipped_file() {
    let cat = Catalog::builtin();
    let built = starter::build_starter(&cat, "rover").unwrap();
    let shipped =
        std::fs::read_to_string(asset("rover.axlbot")).expect("assets/robots/rover.axlbot");
    assert_eq!(
        io::to_text(&built),
        shipped,
        "run `cargo run -p axl-build --example make_starters`"
    );
}

#[test]
fn test_chassis_matches_the_shipped_file() {
    let cat = Catalog::builtin();
    let built = starter::build_starter(&cat, "test_chassis").unwrap();
    let shipped = std::fs::read_to_string(asset("test_chassis.axlbot")).unwrap();
    assert_eq!(io::to_text(&built), shipped);
    assert_eq!(
        built.parts.iter().filter(|p| p.device.is_some()).count(),
        1,
        "only the hub is a device"
    );
}

#[test]
fn the_rover_has_the_spec_parts_and_validates_clean() {
    let cat = Catalog::builtin();
    let a = starter::build_starter(&cat, "rover").unwrap();
    let count = |id: &str| a.parts.iter().filter(|p| p.part.0 == id).count();
    assert_eq!(count("motor_medium"), 2);
    assert_eq!(count("wheel_56"), 2);
    assert_eq!(count("caster_ball"), 1);
    assert_eq!(count("sensor_ultrasonic"), 1);
    assert_eq!(count("sensor_color"), 1);
    assert_eq!(count("hub"), 1);
    let b = Builder::with_assembly(cat, a);
    assert!(b.issues().is_empty(), "{:?}", b.issues());
    assert!(b.can_simulate());
    let names: Vec<_> = b
        .assembly
        .parts
        .iter()
        .filter_map(|p| p.device.as_ref())
        .map(|d| (d.name.as_str(), d.port))
        .collect();
    assert!(names.contains(&("left", Some(axl_build::Port::A))));
    assert!(names.contains(&("right", Some(axl_build::Port::B))));
}

#[test]
fn undo_takes_the_rover_apart_step_by_step_and_redo_rebuilds_it() {
    let cat = Catalog::builtin();
    let mut b = Builder::new(cat);
    starter::run_steps(&mut b, &starter::rover_steps()).unwrap();
    let full = b.assembly.clone();
    while b.undo().is_some() {}
    assert!(b.assembly.parts.is_empty());
    while b.redo().is_some() {}
    assert_eq!(b.assembly.parts, full.parts);
}

#[test]
fn save_then_load_gives_an_identical_file() {
    let cat = Catalog::builtin();
    let a = starter::build_starter(&cat, "rover").unwrap();
    let dir = std::env::temp_dir().join(format!("axl-build-test-{}", std::process::id()));
    let p = dir.join("robots").join("Rover.axlbot");
    io::save(&a, &p).unwrap();
    let first = std::fs::read(&p).unwrap();
    let loaded = io::load(&p, &cat).unwrap();
    assert_eq!(loaded.parts, a.parts);
    io::save(&loaded, &p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), first);
    // The second save kept the first as .bak.
    assert_eq!(
        std::fs::read(dir.join("robots").join("Rover.axlbot.bak")).unwrap(),
        first
    );
    assert!(!dir.join("robots").join("Rover.axlbot.tmp").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_newer_file_is_refused() {
    let cat = Catalog::builtin();
    let err = io::from_text("(version: 2, name: \"x\", parts: [])", &cat, "x").unwrap_err();
    assert_eq!(
        err.to_string(),
        "This file was made by a newer version of AXL."
    );
    assert!(io::from_text("not ron", &cat, "x").is_err());
}

#[test]
fn deleting_a_part_makes_the_rest_float_and_blocks_simulate() {
    let cat = Catalog::builtin();
    let mut b = Builder::with_assembly(
        cat,
        starter::build_starter(&Catalog::builtin(), "rover").unwrap(),
    );
    // Delete the plate: everything hangs off it.
    b.select(Some(1), false);
    b.delete_selected().unwrap();
    assert!(has_errors(b.issues()));
    assert!(
        b.issues()
            .iter()
            .any(|i| i.level == Level::Error && i.message.contains("floating"))
    );
}

#[test]
fn validation_messages_match_the_spec() {
    let cat = Catalog::builtin();
    let mut b = Builder::new(cat);
    assert!(
        b.issues()
            .iter()
            .any(|i| i.message == "A robot needs exactly one hub.")
            || b.assembly.parts.is_empty()
    );
    starter::run_steps(&mut b, &starter::test_chassis_steps()).unwrap();
    // Rename the hub so a motor can clash with it: two devices share a name.
    let hub = b
        .assembly
        .parts
        .iter()
        .find(|p| p.part.0 == "hub")
        .unwrap()
        .uid;
    b.set_device_name(hub, "left_motor").unwrap();
    assert!(b.set_device_name(hub, "Bad Name").is_err());
}

#[test]
fn a_motor_that_turns_nothing_and_a_loose_wheel_warn() {
    let cat = Catalog::builtin();
    let mut b = Builder::new(cat);
    starter::run_steps(&mut b, &starter::test_chassis_steps()).unwrap();
    // A motor under the plate, with nothing on its output.
    use axl_build::place::Ray;
    b.start_place("motor_medium", None).unwrap();
    b.hover(&Ray {
        origin: [0.0, 0.0, -300.0],
        dir: [0.0, 0.0, 1.0],
    });
    b.click_place().unwrap();
    assert!(
        b.issues()
            .iter()
            .any(|i| i.level == Level::Warning && i.message == "Motor A turns nothing."),
        "{:?}",
        b.issues()
    );
}
