//! Every mission against a robot that does nothing, and the C reference solutions against every
//! mission.

use std::path::{Path, PathBuf};
use std::process::Command;

use axl_missions::library::{builtin_missions, mission};
use axl_parts::catalog::Catalog;
use axl_sim::mission::{grade_mission, robot_assembly};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The robot design for a mission: its starter, or the reference design for m12.
fn design(id: &str) -> Option<PathBuf> {
    (id == "m12").then(|| repo().join("missions/solutions/m12.axlbot"))
}

#[test]
fn a_robot_that_does_nothing_fails_every_mission() {
    let cat = Catalog::builtin();
    let missions = builtin_missions().unwrap();
    assert_eq!(missions.len(), 12);
    for m in missions {
        let asm = robot_assembly(&m, &cat, design(&m.id).as_deref()).unwrap();
        let run = grade_mission(&m, &asm, &cat, None, &m.grading_seeds, false).unwrap();
        assert!(!run.grade.passed, "{} passes without a program", m.id);
        assert_eq!(run.grade.stars, 0, "{}", m.id);
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

#[test]
fn c_reference_solutions_pass_every_grading_seed() {
    let Some(sdk) = wasi_sdk() else {
        eprintln!("wasi-sdk not found: skipping (cargo xtask sdk-check --require-c checks this)");
        return;
    };
    let cat = Catalog::builtin();
    let out = std::env::temp_dir().join("axl-c-solutions");
    std::fs::create_dir_all(&out).unwrap();
    for n in 1..=12 {
        let id = format!("m{n:02}");
        let m = mission(&id).unwrap();
        let wasm = out.join(format!("{id}.wasm"));
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
            .arg(&wasm)
            .status()
            .unwrap();
        assert!(status.success(), "{id} does not build");
        let asm = robot_assembly(&m, &cat, design(&id).as_deref()).unwrap();
        let bytes = std::fs::read(&wasm).unwrap();
        let run = grade_mission(&m, &asm, &cat, Some(&bytes), &m.grading_seeds, false).unwrap();
        assert!(run.grade.passed, "{id} fails: {:#?}", run.grade.seeds);
    }
}
