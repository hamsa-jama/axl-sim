//! The live view of a mission runs the mission's own world: the same world, robot, mat, battery
//! and seed as the graded run of that seed, so the learner watches the run that is graded.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use axl_missions::library::{builtin_missions, mission};
use axl_parts::catalog::Catalog;
use axl_sim::mission::{robot_assembly, run_seed, setup};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn design(id: &str) -> Option<PathBuf> {
    (id == "m12").then(|| repo().join("missions/solutions/m12.axlbot"))
}

#[test]
fn every_mission_has_a_live_world_with_its_walls_and_a_robot_that_stands_on_the_floor() {
    let cat = Catalog::builtin();
    for m in builtin_missions().unwrap() {
        let asm = robot_assembly(&m, &cat, design(&m.id).as_deref()).unwrap();
        let s = setup(&m, 1, &asm, &cat).unwrap();
        assert_eq!(s.seed(), 1, "{}", m.id);
        // Everything the physics has, the picture has: every wall and static shape of the world.
        let walls = s.compiled.mjcf.matches("name=\"wall_").count();
        assert_eq!(s.shapes.len(), walls + s.world.statics.len(), "{}", m.id);
        assert_eq!(s.world.id, m.world.id);
        assert!(s.object_body_ids().unwrap().iter().all(Option::is_some));
        // The live sim thread starts in that world and publishes poses of the robot.
        let mut live = s.spawn_live(&m, None, true).unwrap();
        let t0 = Instant::now();
        loop {
            let snap = live.latest();
            if !snap.bodies.is_empty() {
                let z = snap.bodies[snap.root_body].pos[2];
                assert!(z > 0.0 && z < 0.2, "{} root z {z}", m.id);
                break;
            }
            assert!(
                t0.elapsed() < Duration::from_secs(10),
                "{} never published",
                m.id
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn wasi_sdk() -> Option<PathBuf> {
    let mut found = vec![];
    if let Some(p) = std::env::var_os("WASI_SDK_PATH") {
        found.push(PathBuf::from(p));
    }
    found.push(repo().join("third_party/wasi-sdk"));
    found.into_iter().find(|p| p.join("bin/clang").is_file())
}

/// Build the C reference solution of `id`.
fn build_solution(sdk: &Path, id: &str) -> Vec<u8> {
    let out = std::env::temp_dir().join(format!("axl-live-{id}-{}.wasm", std::process::id()));
    let status = Command::new(sdk.join("bin/clang"))
        .arg("--target=wasm32-wasip1")
        .arg(format!(
            "--sysroot={}",
            sdk.join("share/wasi-sysroot").display()
        ))
        .args(["-std=c17", "-O2", "-Wall", "-Wextra"])
        .arg(format!("-I{}", repo().join("sdk/c").display()))
        .arg(repo().join(format!("missions/solutions/c/{id}/main.c")))
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());
    let bytes = std::fs::read(&out).unwrap();
    let _ = std::fs::remove_file(out);
    bytes
}

#[test]
fn the_live_run_of_seed_1_ends_where_the_graded_run_of_seed_1_ends() {
    let Some(sdk) = wasi_sdk() else {
        eprintln!("wasi-sdk not found: skipping");
        return;
    };
    let cat = Catalog::builtin();
    // m01 drives to the 1 m mark and stops; m05 stops at a wall: both end at rest.
    for id in ["m01", "m05"] {
        let m = mission(id).unwrap();
        let wasm = build_solution(&sdk, id);
        let asm = robot_assembly(&m, &cat, design(id).as_deref()).unwrap();
        let (graded, _) = run_seed(&m, &asm, &cat, 1, Some(&wasm), &Default::default()).unwrap();
        let end = graded.trace.last().unwrap();

        let s = setup(&m, 1, &asm, &cat).unwrap();
        let mut live = s.spawn_live(&m, Some(wasm), false).unwrap();
        let t0 = Instant::now();
        let pos = loop {
            let snap = live.latest();
            if snap.time >= end.t - 0.02 && !snap.bodies.is_empty() {
                break snap.bodies[snap.root_body].pos;
            }
            assert!(
                t0.elapsed() < Duration::from_secs(60),
                "{id} did not reach t = {}",
                end.t
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let d = (pos[0] - end.pos[0]).hypot(pos[1] - end.pos[1]);
        assert!(d < 0.002, "{id}: live run ends {d} m from the graded run");
    }
}
