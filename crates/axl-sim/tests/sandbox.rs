use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, PhysicsParams, compile};
use axl_parts::catalog::Catalog;
use axl_sim::{Command, Sandbox, run_headless};

fn rover() -> axl_compile::Compiled {
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    compile(&cat, &asm, &PhysicsParams::builtin(), &CompileOptions::default()).unwrap()
}

#[test]
fn fixed_voltages_drive_the_rover_forward() {
    let c = rover();
    let v = [("left".to_string(), 4.0), ("right".to_string(), 4.0)];
    let s = run_headless(&c, &v, 2.0).unwrap();
    assert!((s.time - 2.0).abs() < 0.002);
    let root = c.bodies.bodies.iter().find(|b| b.name == "root").unwrap().id;
    assert!(s.bodies[root].pos[0] > 0.3, "{:?}", s.bodies[root].pos);
    assert!(s.error.is_none());
}

#[test]
fn an_unknown_motor_is_refused() {
    let c = rover();
    let e = run_headless(&c, &[("nope".into(), 1.0)], 0.1).err().unwrap();
    assert_eq!(e.to_string(), "There is no motor called nope.");
}

#[test]
fn the_sim_thread_publishes_snapshots_and_obeys_commands() {
    let c = rover();
    let mut sb = Sandbox::spawn(&c, &[("left".into(), 4.0), ("right".into(), 4.0)]).unwrap();
    sb.send(Command::SetSpeed(4.0));
    let root = c.bodies.bodies.iter().find(|b| b.name == "root").unwrap().id;
    let start = std::time::Instant::now();
    let mut x = 0.0;
    while start.elapsed().as_secs_f64() < 5.0 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let s = sb.latest();
        if s.bodies.len() > root {
            x = s.bodies[root].pos[0];
        }
        if x > 0.2 {
            break;
        }
    }
    assert!(x > 0.2, "rover moved to x = {x}");
    sb.send(Command::Pause);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let t1 = sb.latest().time;
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(sb.latest().time, t1, "paused sim must not advance");
}
