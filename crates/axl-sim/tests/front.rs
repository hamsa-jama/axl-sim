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

#[test]
fn dbg_trace() {
    let Ok(path) = std::env::var("DBG_WASM") else { return };
    let id = std::env::var("DBG_MISSION").unwrap();
    let m = mission(&id).unwrap();
    let cat = Catalog::builtin();
    let asm = axl_sim::mission::robot_assembly(&m, &cat, None).unwrap();
    let wasm = std::fs::read(path).unwrap();
    let (run, _) = axl_sim::mission::run_seed(&m, &asm, &cat, 1, Some(&wasm), &Default::default()).unwrap();
    for s in run.trace.samples.iter().step_by(std::env::var("DBG_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(50)) {
        eprintln!("t {:.2} x {:.4} y {:.4} yaw {:.3} v {:.3} front {:.3} tilt {:.2} z {:.3}", s.t, s.pos[0], s.pos[1], s.yaw, s.speed, s.front[0], s.tilt, s.pos[2]);
    }
}

#[test]
fn dbg_com() {
    let m = mission("m01").unwrap();
    let cat = Catalog::builtin();
    for id in ["rover"] {
        let asm = build_starter(&cat, id).unwrap();
        let s = setup(&m, 1, &asm, &cat).unwrap();
        let mut core = s.core().unwrap();
        core.tick();
        let root = core.root_body();
        let com = core.physics().subtree_com(root);
        let mass = core.physics().subtree_mass(root);
        eprintln!("{id}: com {com:?} mass {mass}");
        for i in 0..core.physics().body_count() {
            let p = core.physics().body_pose(axl_mujoco::BodyId(i));
            eprintln!("body {i} {:?} mass {}", p.pos, core.physics().body_mass(axl_mujoco::BodyId(i)));
        }
    }
}

#[test]
fn dbg_ramp_contacts() {
    use axl_hw::MotorCommand;
    let m = mission("m07").unwrap();
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    let s = setup(&m, 1, &asm, &cat).unwrap();
    let mut core = s.core().unwrap();
    for t in 0..2500 {
        if t == 0 {
            let _ = core.command("left", MotorCommand::Power(0.4));
            let _ = core.command("right", MotorCommand::Power(0.4));
        }
        core.tick();
        if t % 500 == 499 {
            let root = core.physics().body_pose(core.root_body()).pos;
            eprintln!("t {} root {:?}", t + 1, root);
            for c in core.physics().contacts() {
                if c.normal_force.abs() > 1e-6 {
                    eprintln!(
                        "  rel ({:.3},{:.3},{:.3}) n ({:.2},{:.2},{:.2}) f {:.2} g {:?}/{:?}",
                        c.pos[0] - root[0], c.pos[1] - root[1], c.pos[2], c.normal[0], c.normal[1], c.normal[2], c.normal_force, c.geom1, c.geom2
                    );
                }
            }
        }
    }
}
