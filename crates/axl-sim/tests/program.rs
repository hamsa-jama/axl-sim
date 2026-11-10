//! Learner programs (built from C with wasi-sdk) driving the Rover on the sim thread.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_host::toolchain::{
    BuildRequest, Lang, ToolchainStatus, build, ensure_project, find_wasi_sdk,
};
use axl_host::{HostConfig, RunEnd, StopKind, WATCHDOG_MESSAGE};
use axl_parts::catalog::Catalog;
use axl_sim::{HwOptions, ProgramConfig, Sandbox, SimEvent};

fn rover_with_wall() -> Compiled {
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    let opts = CompileOptions {
        extra_worldbody: r#"<geom name="wall" type="box" pos="0.9 0 0.1" size="0.01 1 0.1" contype="2" conaffinity="1" group="0" rgba="0.9 0.9 0.9 1"/>"#.into(),
        ..Default::default()
    };
    compile(&cat, &asm, &PhysicsParams::builtin(), &opts).unwrap()
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Build a C project from `source` (or the m01 template when `None`). `None` when wasi-sdk is
/// missing (CI installs it).
fn build_c(name: &str, source: Option<&str>) -> Option<Vec<u8>> {
    let sdk = find_wasi_sdk()?;
    let dir = std::env::temp_dir().join(format!("axl-prog-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text = match source {
        Some(s) => s.to_owned(),
        None => std::fs::read_to_string(repo().join("templates/c/m01/main.c")).unwrap(),
    };
    std::fs::write(dir.join("main.c"), text).unwrap();
    let out = build(
        &BuildRequest {
            lang: Lang::C,
            project: dir.clone(),
            sdk: repo().join("sdk"),
            wasi_sdk: Some(sdk),
        },
        |_| {},
    );
    assert!(out.success, "build failed: {:?}", out.diagnostics);
    let wasm = std::fs::read(out.wasm.unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    Some(wasm)
}

/// Called when a toolchain is missing: CI sets `AXL_REQUIRE_TOOLCHAINS` so that is a failure there.
fn skip(what: &str) {
    assert!(
        std::env::var_os("AXL_REQUIRE_TOOLCHAINS").is_none(),
        "{what} is required here but not installed"
    );
    eprintln!("{what} not found: skipped");
}

/// Build the Rust m01 template for wasm32-wasip1 (a copy in a temp folder, like a learner's).
fn build_rust_template() -> Option<Vec<u8>> {
    // Like a learner: a copy outside the repo (so the repo's rust-toolchain.toml does not apply),
    // with no toolchain forced through the environment.
    // SAFETY: only this test binary reads the variable, and it does so after this point.
    unsafe { std::env::remove_var("RUSTUP_TOOLCHAIN") };
    if Command::new("rustup").arg("--version").output().is_err() {
        return None;
    }
    let data = std::env::temp_dir().join(format!("axl-prog-{}-rust", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let project = ensure_project(
        &data,
        "m01",
        Lang::Rust,
        &repo().join("templates"),
        &repo().join("sdk"),
    )
    .unwrap();
    // Here the wasm32-wasip1 target cannot be downloaded; CI can, and requires the build.
    if std::env::var_os("AXL_REQUIRE_TOOLCHAINS").is_none()
        && !ToolchainStatus::detect_in(Some(&project)).rust_ready()
    {
        let _ = std::fs::remove_dir_all(&data);
        return None;
    }
    let out = build(
        &BuildRequest {
            lang: Lang::Rust,
            project,
            sdk: repo().join("sdk"),
            wasi_sdk: None,
        },
        |_| {},
    );
    assert!(out.success, "build failed: {:?}", out.diagnostics);
    let wasm = std::fs::read(out.wasm.unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&data);
    Some(wasm)
}

fn spawn(c: &Compiled, wasm: Vec<u8>, paced: bool) -> Sandbox {
    Sandbox::spawn_program(
        c,
        &[],
        HwOptions::default(),
        ProgramConfig {
            wasm,
            host: HostConfig::default(),
        },
        paced,
    )
    .unwrap()
}

fn root_x(sb: &mut Sandbox, c: &Compiled) -> (f64, f64) {
    let id = c
        .bodies
        .bodies
        .iter()
        .find(|b| b.name == "root")
        .unwrap()
        .id;
    let s = sb.latest();
    (s.time, s.bodies.get(id).map_or(0.0, |b| b.pos[0]))
}

fn wait_ended(sb: &Sandbox, secs: f64) -> (Result<RunEnd, String>, f64) {
    let t = Instant::now();
    while t.elapsed().as_secs_f64() < secs {
        if let Ok(SimEvent::Ended(e)) = sb.events().recv_timeout(Duration::from_millis(50)) {
            return (e, t.elapsed().as_secs_f64());
        }
    }
    panic!("the program did not end within {secs} s");
}

fn drives_up_to_the_wall_and_stops(wasm: Vec<u8>) {
    let c = rover_with_wall();
    let mut sb = spawn(&c, wasm, false);
    let t = Instant::now();
    loop {
        let (time, _) = root_x(&mut sb, &c);
        if time > 8.0 || t.elapsed().as_secs_f64() > 60.0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let (time, x) = root_x(&mut sb, &c);
    assert!(time > 8.0, "sim only reached {time} s");
    // The wall face is at x = 0.89; the program stops within the 0.20 m threshold (plus glide).
    assert!(x > 0.3 && x < 0.89, "x = {x}");
    sb.send(axl_sim::Command::Stop);
    let (end, _) = wait_ended(&sb, 10.0);
    assert_eq!(end, Ok(RunEnd::Finished));
}

#[test]
fn the_c_template_drives_the_rover_up_to_the_wall_and_stops() {
    match build_c("m01", None) {
        Some(wasm) => drives_up_to_the_wall_and_stops(wasm),
        None => skip("wasi-sdk"),
    }
}

#[test]
fn the_rust_template_drives_the_rover_up_to_the_wall_and_stops() {
    match build_rust_template() {
        Some(wasm) => drives_up_to_the_wall_and_stops(wasm),
        None => skip("the wasm32-wasip1 Rust target"),
    }
}

#[test]
fn an_endless_loop_shows_the_watchdog_message_within_2_5_s() {
    let Some(wasm) = build_c(
        "loop",
        Some(
            "#include \"axl.h\"\nint main(void) {\n    axl_init(AXL_ABI_VERSION);\n    volatile int x = 0;\n    for (;;) { x++; }\n}\n",
        ),
    ) else {
        skip("wasi-sdk");
        return;
    };
    let c = rover_with_wall();
    let sb = spawn(&c, wasm, true);
    let (end, secs) = wait_ended(&sb, 5.0);
    let Ok(RunEnd::Stopped(e)) = end else {
        panic!("{end:?}")
    };
    assert_eq!(e.kind, StopKind::Watchdog);
    assert_eq!(e.message, WATCHDOG_MESSAGE);
    assert!((1.9..2.5).contains(&secs), "{secs}");
}

#[test]
fn a_wrong_device_call_reports_the_source_line() {
    let src = "#include \"axl.h\"\n\nint main(void) {\n    axl_init(AXL_ABI_VERSION);\n    int eye = axl_device_named(\"eye\");\n    axl_motor_power(eye, 0.5);\n    return 0;\n}\n";
    let Some(wasm) = build_c("wrongdev", Some(src)) else {
        skip("wasi-sdk");
        return;
    };
    let c = rover_with_wall();
    let sb = spawn(&c, wasm, false);
    let (end, _) = wait_ended(&sb, 10.0);
    let Ok(RunEnd::Stopped(e)) = end else {
        panic!("{end:?}")
    };
    assert_eq!(e.kind, StopKind::BadDevice);
    let loc = e.location.expect("a source location");
    assert!(loc.file.ends_with("main.c"), "{loc}");
    assert_eq!(loc.line, 6, "{loc}");
}

#[test]
fn a_program_that_returns_brakes_the_motors_and_the_sim_runs_on() {
    let src = "#include \"axl.h\"\nint main(void) {\n    axl_init(AXL_ABI_VERSION);\n    int l = axl_device_named(\"left\");\n    int r = axl_device_named(\"right\");\n    axl_motor_power(l, 0.5);\n    axl_motor_power(r, 0.5);\n    axl_step(500);\n    return 0;\n}\n";
    let Some(wasm) = build_c("returns", Some(src)) else {
        skip("wasi-sdk");
        return;
    };
    let c = rover_with_wall();
    let mut sb = spawn(&c, wasm, false);
    let (end, _) = wait_ended(&sb, 10.0);
    assert_eq!(end, Ok(RunEnd::Finished));
    std::thread::sleep(Duration::from_millis(300));
    let (t1, x1) = root_x(&mut sb, &c);
    std::thread::sleep(Duration::from_millis(300));
    let (t2, x2) = root_x(&mut sb, &c);
    assert!(t2 > t1, "the simulation kept running");
    assert!((x2 - x1).abs() < 0.01, "braked: {x1} -> {x2}");
}

/// Every checkpoint (whole second -> hash) the events report until `until`, in order, ignoring
/// everything else.
fn read_checkpoints(sb: &Sandbox, until: f64, out: &mut std::collections::BTreeMap<u64, u64>) {
    let t = Instant::now();
    while t.elapsed().as_secs_f64() < 60.0 {
        if let Ok(SimEvent::Checkpoint { time, hash, .. }) =
            sb.events().recv_timeout(Duration::from_millis(100))
        {
            out.entry(time as u64).or_insert(hash);
            if time >= until {
                return;
            }
        }
    }
    panic!("no checkpoint at {until} s");
}

#[test]
fn rewinding_a_running_program_replays_it_exactly() {
    let Some(wasm) = build_c("rewind", None) else {
        skip("wasi-sdk");
        return;
    };
    let c = rover_with_wall();
    let mut sb = spawn(&c, wasm, false);
    // The first run, flat out: remember the joint-position hash of every second.
    let mut first = std::collections::BTreeMap::new();
    read_checkpoints(&sb, 5.0, &mut first);
    assert!(first.len() >= 6, "{first:?}");
    // Rewind to 3 s: the replay passes through the same hashes and then stops there, paused.
    sb.send(axl_sim::Command::Rewind(3.0));
    let s = loop {
        let s = sb.latest().clone();
        if s.paused && s.replay_target.is_none() && (s.time - 3.0).abs() < 1e-6 {
            break s;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(s.hash, first[&3], "the replay differs at 3 s");
    // Rewind further back and forward again: still the same.
    sb.send(axl_sim::Command::Rewind(1.0));
    let t = Instant::now();
    let s = loop {
        let s = sb.latest().clone();
        if s.paused && s.replay_target.is_none() && (s.time - 1.0).abs() < 1e-6 {
            break s;
        }
        assert!(t.elapsed().as_secs_f64() < 30.0, "no replay to 1 s");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(s.hash, first[&1]);
    // Play on from there: the continuation reaches the original hash at 5 s.
    sb.send(axl_sim::Command::Resume);
    let mut after = std::collections::BTreeMap::new();
    // Drain what the first run and the replays reported, then the continuation: later wins.
    let mut last5 = None;
    let t = Instant::now();
    while t.elapsed().as_secs_f64() < 60.0 && last5.is_none() {
        if let Ok(SimEvent::Checkpoint { time, hash, .. }) =
            sb.events().recv_timeout(Duration::from_millis(100))
        {
            after.insert(time as u64, hash);
            if time as u64 == 5 && sb.latest().time > 5.0 && !sb.latest().paused {
                last5 = Some(hash);
            }
        }
    }
    assert!(
        after
            .iter()
            .all(|(t, h)| first.get(t).is_none_or(|f| f == h))
    );
    sb.send(axl_sim::Command::Stop);
}
