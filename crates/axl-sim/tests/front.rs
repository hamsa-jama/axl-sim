//! The recorder finds the front of the robot.

use axl_build::starter::build_starter;
use axl_missions::library::mission;
use axl_parts::catalog::Catalog;
use axl_sim::mission::setup;

#[test]
fn front_of_the_rover() {
    let m = mission("m01").unwrap();
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    let s = setup(&m, 1, &asm, &cat).unwrap();
    let mut core = s.core().unwrap();
    let mut rec = s.recorder();
    rec.start(&mut core);
    let first = rec.samples[0];
    eprintln!("pos {:?} front {:?}", first.pos, first.front);
    assert!(first.front[0] - first.pos[0] > 0.02);
}
