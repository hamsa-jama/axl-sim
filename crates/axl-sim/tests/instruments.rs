//! Time controls, rewind by deterministic replay, picking and the mouse drag on the sim thread.

use std::time::{Duration, Instant};

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_parts::catalog::Catalog;
use axl_sim::{Command, Sandbox, SimCore, SimEvent, SimSnapshot, joint_hash};

fn rover() -> Compiled {
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    compile(
        &cat,
        &asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .unwrap()
}

fn volts() -> Vec<(String, f64)> {
    vec![("left".into(), 4.0), ("right".into(), 3.0)]
}

/// Wait until `f` is true of the newest snapshot, or fail after `secs`.
fn wait_for(
    sb: &mut Sandbox,
    secs: f64,
    what: &str,
    f: impl Fn(&SimSnapshot) -> bool,
) -> SimSnapshot {
    let start = Instant::now();
    loop {
        let s = sb.latest().clone();
        if f(&s) {
            return s;
        }
        assert!(start.elapsed().as_secs_f64() < secs, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The hash the checkpoint event at `time` carries.
fn checkpoint_hash(sb: &Sandbox, time: f64) -> u64 {
    let start = Instant::now();
    loop {
        match sb.events().recv_timeout(Duration::from_millis(200)) {
            Ok(SimEvent::Checkpoint { time: t, hash, .. }) if (t - time).abs() < 1e-9 => {
                return hash;
            }
            _ => {}
        }
        assert!(
            start.elapsed().as_secs_f64() < 30.0,
            "no checkpoint at {time} s"
        );
    }
}

#[test]
fn a_reset_core_replays_the_same_joint_positions() {
    let c = rover();
    let mut core = SimCore::new(&c, &volts()).unwrap();
    let run_to = |core: &mut SimCore, t: f64| {
        while core.time() < t - 1e-9 {
            assert!(core.tick());
        }
        joint_hash(core.physics())
    };
    let first: Vec<u64> = [0.5, 1.25, 3.0]
        .iter()
        .map(|t| run_to(&mut core, *t))
        .collect();
    core.reset();
    let second: Vec<u64> = [0.5, 1.25, 3.0]
        .iter()
        .map(|t| run_to(&mut core, *t))
        .collect();
    assert_eq!(first, second);
    assert!(
        first[0] != first[1] && first[1] != first[2],
        "the robot moved"
    );
}

#[test]
fn rewinding_to_a_checkpoint_gives_the_same_joint_hash() {
    let c = rover();
    let mut sb = Sandbox::spawn(&c, &volts()).unwrap();
    sb.send(Command::SetSpeed(4.0));
    let want = checkpoint_hash(&sb, 3.0);
    sb.send(Command::Rewind(3.0));
    let s = wait_for(&mut sb, 30.0, "replay to 3 s", |s| {
        s.paused && s.replay_target.is_none() && (s.time - 3.0).abs() < 1e-6
    });
    assert_eq!(s.hash, want);
}

#[test]
fn rewinding_to_a_time_between_checkpoints_matches_a_straight_run() {
    let c = rover();
    let mut core = SimCore::new(&c, &volts()).unwrap();
    while core.time() < 1.5 - 1e-9 {
        core.tick();
    }
    let want = joint_hash(core.physics());
    let mut sb = Sandbox::spawn(&c, &volts()).unwrap();
    sb.send(Command::SetSpeed(4.0));
    wait_for(&mut sb, 30.0, "run past 2 s", |s| s.time > 2.0);
    sb.send(Command::Rewind(1.5));
    let s = wait_for(&mut sb, 30.0, "replay to 1.5 s", |s| {
        s.paused && s.replay_target.is_none() && (s.time - 1.5).abs() < 1e-6
    });
    assert_eq!(s.hash, want);
    // And it stays there: paused.
    std::thread::sleep(Duration::from_millis(150));
    assert!((sb.latest().time - 1.5).abs() < 1e-6);
}

#[test]
fn a_replay_applies_the_logged_drag_again() {
    let c = rover();
    let root = c.bodies.bodies[0].id;
    let mut sb = Sandbox::spawn(&c, &volts()).unwrap();
    sb.send(Command::SetSpeed(4.0));
    let s = wait_for(&mut sb, 30.0, "first poses", |s| s.time > 0.3);
    let p = s.bodies[root].pos;
    sb.send(Command::ApplyDragForce {
        body: root,
        point: p,
        force: [0.0, 1.5, 0.0],
    });
    let want = checkpoint_hash(&sb, 3.0);
    sb.send(Command::ClearDrag);
    sb.send(Command::Rewind(3.0));
    let s = wait_for(&mut sb, 30.0, "replay to 3 s", |s| {
        s.paused && s.replay_target.is_none() && (s.time - 3.0).abs() < 1e-6
    });
    assert_eq!(s.hash, want, "the drag was not replayed");
    // Without the drag the run differs, so the check above means something.
    let mut plain = SimCore::new(&c, &volts()).unwrap();
    while plain.time() < 3.0 - 1e-9 {
        plain.tick();
    }
    assert_ne!(joint_hash(plain.physics()), want);
}

#[test]
fn pause_step_and_speed() {
    let c = rover();
    let mut sb = Sandbox::spawn(&c, &volts()).unwrap();
    wait_for(&mut sb, 10.0, "running", |s| s.time > 0.05);
    sb.send(Command::Pause);
    let s = wait_for(&mut sb, 5.0, "paused flag", |s| s.paused);
    std::thread::sleep(Duration::from_millis(100));
    let t0 = sb.latest().time;
    assert!(t0 >= s.time);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(sb.latest().time, t0, "time moved while paused");
    sb.send(Command::Step { ms: 1 });
    let s1 = wait_for(&mut sb, 5.0, "one step", |s| s.time > t0 + 1e-6);
    assert!(
        (s1.time - (t0 + 0.001)).abs() < 1e-9,
        "{} vs {}",
        s1.time,
        t0
    );
    sb.send(Command::Step { ms: 10 });
    let s2 = wait_for(&mut sb, 5.0, "ten steps", |s| s.time > s1.time + 0.0099);
    assert!((s2.time - (s1.time + 0.010)).abs() < 1e-9);
    assert!(s2.paused);
    sb.send(Command::SetSpeed(9.0));
    let s3 = wait_for(&mut sb, 5.0, "speed", |s| s.speed > 3.0);
    assert_eq!(s3.speed, 4.0, "clamped to 4x");
    sb.send(Command::SetSpeed(0.01));
    wait_for(&mut sb, 5.0, "speed", |s| s.speed < 0.5);
    assert_eq!(sb.latest().speed, 0.1, "clamped to 0.1x");
    sb.send(Command::SetSpeed(4.0));
    sb.send(Command::Resume);
    wait_for(&mut sb, 10.0, "running again", |s| {
        !s.paused && s.time > s2.time + 0.2
    });
}

#[test]
fn reset_goes_back_to_the_start_and_keeps_the_voltages() {
    let c = rover();
    let root = c.bodies.bodies[0].id;
    let mut sb = Sandbox::spawn(&c, &volts()).unwrap();
    sb.send(Command::SetSpeed(4.0));
    let s = wait_for(&mut sb, 20.0, "moving", |s| s.time > 1.0);
    let moved = s.bodies[root].pos;
    sb.send(Command::Reset);
    let s0 = wait_for(&mut sb, 10.0, "reset", |s| s.time < 0.2);
    assert!(s0.bodies[root].pos[0] < moved[0]);
    // The voltages were applied again: it drives again.
    let s2 = wait_for(&mut sb, 20.0, "driving again", |s| s.time > 1.0);
    assert!(s2.bodies[root].pos[0] > 0.05);
}

#[test]
fn a_pick_ray_finds_the_robot_and_not_the_floor() {
    let c = rover();
    let root = c.bodies.bodies[0].id;
    let mut sb = Sandbox::spawn(&c, &[]).unwrap();
    wait_for(&mut sb, 10.0, "first snapshot", |s| !s.bodies.is_empty());
    let p = sb.latest().bodies[root].pos;
    sb.send(Command::Pick {
        id: 7,
        origin: [p[0], p[1], p[2] + 1.0],
        dir: [0.0, 0.0, -1.0],
    });
    sb.send(Command::Pick {
        id: 8,
        origin: [3.0, 3.0, 1.0],
        dir: [0.0, 0.0, -1.0],
    });
    let (mut a, mut b) = (None, None);
    let start = Instant::now();
    while (a.is_none() || b.is_none()) && start.elapsed().as_secs_f64() < 10.0 {
        if let Ok(SimEvent::Picked { id, hit }) =
            sb.events().recv_timeout(Duration::from_millis(50))
        {
            match id {
                7 => a = Some(hit),
                8 => b = Some(hit),
                _ => {}
            }
        }
    }
    let hit = a.expect("answer 7").expect("the robot is under the ray");
    assert!(c.bodies.bodies.iter().any(|b| b.id == hit.body));
    assert!(hit.point[2] > 0.01, "hit above the floor: {:?}", hit.point);
    assert!(
        b.expect("answer 8").is_none(),
        "empty floor is not pickable"
    );
}

#[test]
fn a_drag_force_moves_the_body_and_clearing_lets_go() {
    let c = rover();
    let root = c.bodies.bodies[0].id;
    let mut sb = Sandbox::spawn(&c, &[]).unwrap();
    sb.send(Command::SetSpeed(4.0));
    let s = wait_for(&mut sb, 10.0, "first snapshot", |s| s.time > 0.2);
    let p = s.bodies[root].pos;
    sb.send(Command::ApplyDragForce {
        body: root,
        point: p,
        force: [0.0, 15.0, 0.0],
    });
    let s = wait_for(&mut sb, 20.0, "pushed", |s| {
        s.bodies[root].pos[1] > p[1] + 0.02
    });
    sb.send(Command::ClearDrag);
    let t = s.time;
    wait_for(&mut sb, 10.0, "later", |s| s.time > t + 0.5);
    // Without the force the sideways acceleration is gone: it no longer speeds up in y.
    let v1 = sb.latest().body_info[root].velocity[1];
    let t1 = sb.latest().time;
    wait_for(&mut sb, 10.0, "later still", |s| s.time > t1 + 0.5);
    let v2 = sb.latest().body_info[root].velocity[1];
    assert!(v2 <= v1 + 0.05, "still being pushed: {v1} then {v2}");
}

#[test]
fn snapshots_carry_what_the_overlays_and_the_plotter_need() {
    let c = rover();
    let mut sb = Sandbox::spawn(&c, &volts()).unwrap();
    sb.send(Command::SetSpeed(4.0));
    wait_for(&mut sb, 20.0, "driving", |s| s.time > 1.0);
    // A resting robot bounces by micrometres, so some instants have no contact at all: wait for one.
    let s = wait_for(&mut sb, 20.0, "a floor contact", |s| {
        s.time > 1.0 && s.contacts.iter().any(|c| c.floor && c.normal_force > 0.0)
    });
    assert!(s.robot_mass > 0.1);
    assert!(s.com[2] > 0.0 && s.com[2] < 0.2);
    assert!(s.joints.len() >= 2, "wheel hinges: {}", s.joints.len());
    assert!(s.contacts.iter().any(|c| c.floor));
    assert_eq!(s.channel_names.len(), s.channel_values.len());
    assert!(s.channel_names.iter().any(|n| n == "battery (V)"));
    assert!(s.channel_names.iter().any(|n| n == "true x (m)"));
    assert!(s.sensors.len() >= 2, "ultrasonic and colour sensor");
    assert!(s.body_info[s.root_body].mass > 0.0);
    // The wheels turn: some joint has a speed.
    assert!(s.joints.iter().any(|j| j.speed.abs() > 1.0));
}

#[test]
fn the_mouse_spring_pulls_towards_the_target_and_replays_exactly() {
    let c = rover();
    let root = c.bodies.bodies[0].id;
    let mut sb = Sandbox::spawn(&c, &[]).unwrap();
    sb.send(Command::SetSpeed(4.0));
    let s = wait_for(&mut sb, 10.0, "first snapshot", |s| s.time > 0.2);
    let p = s.bodies[root].pos;
    let target = [p[0], p[1] + 0.20, p[2]];
    sb.send(Command::DragTo {
        body: root,
        local: [0.0; 3],
        target,
    });
    // The spring is slow compared with the frame rate it is driven at: it never overshoots wildly.
    let s = wait_for(&mut sb, 20.0, "pulled", |s| {
        s.bodies[root].pos[1] > p[1] + 0.08
    });
    assert!(
        s.bodies[root].pos[1] < p[1] + 0.3,
        "flung: {:?}",
        s.bodies[root].pos
    );
    let want = checkpoint_hash(&sb, 3.0);
    sb.send(Command::ClearDrag);
    sb.send(Command::Rewind(3.0));
    let s = wait_for(&mut sb, 30.0, "replay to 3 s", |s| {
        s.paused && s.replay_target.is_none() && (s.time - 3.0).abs() < 1e-6
    });
    assert_eq!(s.hash, want, "the spring was not replayed");
}
