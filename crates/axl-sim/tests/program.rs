//! Learner programs (built from C with wasi-sdk) driving the Rover on the sim thread.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_host::toolchain::{BuildRequest, Lang, build, find_wasi_sdk};
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

#[test]
fn the_c_template_drives_the_rover_up_to_the_wall_and_stops() {
    let Some(wasm) = build_c("m01", None) else {
        eprintln!("wasi-sdk not found: skipped");
        return;
    };
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
fn an_endless_loop_shows_the_watchdog_message_within_2_5_s() {
    let Some(wasm) = build_c(
        "loop",
        Some(
            "#include \"axl.h\"\nint main(void) {\n    axl_init(AXL_ABI_VERSION);\n    volatile int x = 0;\n    for (;;) { x++; }\n}\n",
        ),
    ) else {
        eprintln!("wasi-sdk not found: skipped");
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
        eprintln!("wasi-sdk not found: skipped");
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
        eprintln!("wasi-sdk not found: skipped");
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
