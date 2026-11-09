//! WAT tests: every ABI function is called from a hand-written WebAssembly module and checked
//! against a recording robot; then the error messages, the watchdog and the WASI shims.

mod common;

use std::time::{Duration, Instant};

use axl_host::{Controller, HostConfig, LoadError, LogStream, RunEnd, StopKind};
use common::Recorder;

const IMPORTS: &str = r#"
  (import "axl" "init" (func $init (param i32) (result i32)))
  (import "axl" "step" (func $step (param i32) (result i32)))
  (import "axl" "time" (func $time (result f64)))
  (import "axl" "device" (func $device (param i32 i32) (result i32)))
  (import "axl" "device_kind" (func $device_kind (param i32) (result i32)))
  (import "axl" "motor_power" (func $motor_power (param i32 f64)))
  (import "axl" "motor_speed" (func $motor_speed (param i32 f64)))
  (import "axl" "motor_position" (func $motor_position (param i32 f64 f64)))
  (import "axl" "motor_brake" (func $motor_brake (param i32)))
  (import "axl" "motor_coast" (func $motor_coast (param i32)))
  (import "axl" "motor_reset_angle" (func $motor_reset_angle (param i32)))
  (import "axl" "motor_angle" (func $motor_angle (param i32) (result f64)))
  (import "axl" "motor_rate" (func $motor_rate (param i32) (result f64)))
  (import "axl" "motor_current" (func $motor_current (param i32) (result f64)))
  (import "axl" "motor_done" (func $motor_done (param i32) (result i32)))
  (import "axl" "distance" (func $distance (param i32) (result f64)))
  (import "axl" "color_rgb" (func $color_rgb (param i32 i32)))
  (import "axl" "color_reflect" (func $color_reflect (param i32) (result i32)))
  (import "axl" "color_name" (func $color_name (param i32) (result i32)))
  (import "axl" "touch_pressed" (func $touch_pressed (param i32) (result i32)))
  (import "axl" "touch_force" (func $touch_force (param i32) (result f64)))
  (import "axl" "imu_gyro" (func $imu_gyro (param i32)))
  (import "axl" "imu_accel" (func $imu_accel (param i32)))
  (import "axl" "battery_voltage" (func $battery_voltage (result f64)))
  (import "axl" "plot" (func $plot (param i32 i32 f64)))
  (import "axl" "mark_pose" (func $mark_pose (param f64 f64 f64)))
  (import "axl" "draw_line" (func $draw_line (param f64 f64 f64 f64 f64 f64 i32)))
  (import "axl" "draw_clear" (func $draw_clear))
  (import "axl" "report" (func $report (param i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "lefteyefloorbumpnopeplot")
"#;

fn module(body: &str) -> String {
    format!("(module {IMPORTS} (func (export \"_start\") {body}))")
}

fn run_with(wat: &str, cfg: &HostConfig, api: Recorder) -> (RunEnd, Controller<Recorder>) {
    let mut c = Controller::new(wat.as_bytes(), api, cfg).expect("load");
    let end = c.run();
    (end, c)
}

fn run(wat: &str) -> (RunEnd, Controller<Recorder>) {
    run_with(wat, &HostConfig::default(), Recorder::default())
}

fn stopped(end: &RunEnd) -> (&StopKind, &str) {
    match end {
        RunEnd::Stopped(e) => (&e.kind, e.message.as_str()),
        RunEnd::Finished => panic!("expected the program to be stopped"),
    }
}

/// Plot the f64 on the stack under channel "plot" (memory 19..23 holds the name "plot").
const PLOT: &str = "(call $plot (i32.const 20) (i32.const 4)";

#[test]
fn every_abi_function_is_called_and_its_values_arrive() {
    // "left" at 0 (4 bytes), "eye" at 4 (3), "floor" at 7 (5), "bump" at 12 (4).
    let body = format!(
        r#"
    (local $left i32) (local $eye i32) (local $floor i32) (local $bump i32)
    (drop (call $init (i32.const 1)))
    (local.set $left (call $device (i32.const 0) (i32.const 4)))
    (local.set $eye (call $device (i32.const 4) (i32.const 3)))
    (local.set $floor (call $device (i32.const 7) (i32.const 5)))
    (local.set $bump (call $device (i32.const 12) (i32.const 4)))
    ;; handles and kinds
    {PLOT} (f64.convert_i32_s (local.get $left)))
    {PLOT} (f64.convert_i32_s (local.get $eye)))
    {PLOT} (f64.convert_i32_s (local.get $floor)))
    {PLOT} (f64.convert_i32_s (local.get $bump)))
    {PLOT} (f64.convert_i32_s (call $device (i32.const 16) (i32.const 4))))
    {PLOT} (f64.convert_i32_s (call $device_kind (local.get $left))))
    {PLOT} (f64.convert_i32_s (call $device_kind (local.get $eye))))
    {PLOT} (f64.convert_i32_s (call $device_kind (local.get $floor))))
    {PLOT} (f64.convert_i32_s (call $device_kind (local.get $bump))))
    {PLOT} (f64.convert_i32_s (call $device_kind (i32.const 99))))
    ;; lifecycle
    {PLOT} (f64.convert_i32_s (call $step (i32.const 10))))
    {PLOT} (call $time))
    ;; motor commands
    (call $motor_power (local.get $left) (f64.const 0.5))
    (call $motor_power (local.get $left) (f64.const 3.0))
    (call $motor_speed (local.get $left) (f64.const 2.0))
    (call $motor_position (local.get $left) (f64.const 1.0) (f64.const 3.0))
    (call $motor_brake (local.get $left))
    (call $motor_coast (local.get $left))
    (call $motor_reset_angle (local.get $left))
    ;; motor readings
    {PLOT} (call $motor_angle (local.get $left)))
    {PLOT} (call $motor_rate (local.get $left)))
    {PLOT} (call $motor_current (local.get $left)))
    {PLOT} (f64.convert_i32_s (call $motor_done (local.get $left))))
    ;; sensors
    {PLOT} (call $distance (local.get $eye)))
    (call $color_rgb (local.get $floor) (i32.const 100))
    {PLOT} (f64.convert_i32_s (i32.load (i32.const 100))))
    {PLOT} (f64.convert_i32_s (i32.load (i32.const 104))))
    {PLOT} (f64.convert_i32_s (i32.load (i32.const 108))))
    {PLOT} (f64.convert_i32_s (call $color_reflect (local.get $floor))))
    {PLOT} (f64.convert_i32_s (call $color_name (local.get $floor))))
    {PLOT} (f64.convert_i32_s (call $touch_pressed (local.get $bump))))
    {PLOT} (call $touch_force (local.get $bump)))
    ;; hub
    (call $imu_gyro (i32.const 200))
    {PLOT} (f64.load (i32.const 200)))
    {PLOT} (f64.load (i32.const 208)))
    {PLOT} (f64.load (i32.const 216)))
    (call $imu_accel (i32.const 300))
    {PLOT} (f64.load (i32.const 300)))
    {PLOT} (f64.load (i32.const 308)))
    {PLOT} (f64.load (i32.const 316)))
    {PLOT} (call $battery_voltage))
    ;; teaching and missions
    (call $mark_pose (f64.const 0.1) (f64.const 0.2) (f64.const 0.3))
    (call $draw_line (f64.const 0) (f64.const 0) (f64.const 0) (f64.const 1) (f64.const 2) (f64.const 3) (i32.const 0xff0000ff))
    (call $draw_clear)
    (call $report (i32.const 16) (i32.const 4) (f64.const 0.5))
  "#
    );
    let (end, c) = run(&module(&body));
    assert_eq!(end, RunEnd::Finished);
    let api = c.api();
    let plots: Vec<f64> = api.plots.iter().map(|p| p.1).collect();
    assert_eq!(
        plots,
        vec![
            1.0,
            3.0,
            4.0,
            5.0,
            -1.0, // handles; "nope"-like name (16..20 = "nope") is -1
            1.0,
            2.0,
            3.0,
            4.0,
            -1.0, // kinds: motor, distance, colour, touch, bad handle
            0.0,
            12.5 + 0.010, // step returned running, time
            1.25,
            2.5,
            0.75,
            1.0, // motor readings
            0.321,
            100.0,
            200.0,
            300.0,
            42.0,
            3.0, // distance, rgb, reflect, name (red = 3)
            1.0,
            2.5, // touch
            1.0,
            2.0,
            3.0,
            4.0,
            5.0,
            6.0,
            7.7, // gyro, accel, battery
        ]
    );
    assert_eq!(
        api.calls,
        vec![
            "step 10",
            "motor 1 Power(0.5)",
            "motor 1 Power(1.0)",
            "motor 1 Speed(2.0)",
            "motor 1 Position { target: 1.0, max_speed: 3.0 }",
            "motor 1 Brake",
            "motor 1 Coast",
            "reset_angle 1",
            "mark_pose 0.1 0.2 0.3",
            "draw_line [0.0, 0.0, 0.0] [1.0, 2.0, 3.0] 0xff0000ff",
            "draw_clear",
            "report nope 0.5",
        ]
    );
    assert!(api.ended, "program_ended runs when main returns");
}

#[test]
fn the_first_axl_call_must_be_init() {
    let (end, c) = run(&module("(call $motor_power (i32.const 1) (f64.const 0.5))"));
    let (kind, msg) = stopped(&end);
    assert_eq!(*kind, StopKind::NotInitialised);
    assert_eq!(msg, "Call axl_init before anything else.");
    assert!(c.api().calls.is_empty() && c.api().ended);
}

#[test]
fn init_with_a_wrong_abi_version_is_refused() {
    let (end, _) = run(&module("(drop (call $init (i32.const 2)))"));
    let (kind, msg) = stopped(&end);
    assert_eq!(*kind, StopKind::AbiVersion);
    assert!(
        msg.contains("version 2") && msg.contains("version 1"),
        "{msg}"
    );
}

#[test]
fn a_device_of_the_wrong_kind_stops_the_program_with_a_plain_message() {
    let body =
        "(drop (call $init (i32.const 1))) (call $motor_power (i32.const 3) (f64.const 0.5))";
    let (end, _) = run(&module(body));
    let (kind, msg) = stopped(&end);
    assert_eq!(*kind, StopKind::BadDevice);
    assert_eq!(
        msg,
        "axl_motor_power: device 3 (eye) is a distance sensor, not a motor."
    );
    // The other direction, and a handle that does not exist.
    let (end, _) = run(&module(
        "(drop (call $init (i32.const 1))) (drop (call $distance (i32.const 1)))",
    ));
    assert_eq!(
        stopped(&end).1,
        "axl_distance: device 1 (left) is a motor, not a distance sensor."
    );
    let (end, _) = run(&module(
        "(drop (call $init (i32.const 1))) (drop (call $touch_force (i32.const 42)))",
    ));
    assert!(
        stopped(&end)
            .1
            .starts_with("axl_touch_force: device 42 does not exist.")
    );
    let (end, _) = run(&module(
        "(drop (call $init (i32.const 1))) (drop (call $touch_force (i32.const -1)))",
    ));
    assert!(
        stopped(&end)
            .1
            .starts_with("axl_touch_force: device -1 does not exist.")
    );
}

#[test]
fn a_disabled_function_stops_the_program_with_the_mission_message() {
    let cfg = HostConfig {
        disabled_api: vec!["motor_speed".into()],
        ..Default::default()
    };
    let body = "(drop (call $init (i32.const 1))) (call $motor_power (i32.const 1) (f64.const 0.5)) (call $motor_speed (i32.const 1) (f64.const 2.0))";
    let (end, c) = run_with(&module(body), &cfg, Recorder::default());
    let (kind, msg) = stopped(&end);
    assert_eq!(*kind, StopKind::DisabledApi);
    assert_eq!(
        msg,
        "This mission switches off motor_speed so you can build your own controller."
    );
    // The enabled call before it went through; the disabled one did not.
    assert_eq!(c.api().calls, vec!["motor 1 Power(0.5)"]);
}

#[test]
fn bad_arguments_are_explained() {
    let init = "(drop (call $init (i32.const 1)))";
    let (end, _) = run(&module(&format!(
        "{init} (drop (call $step (i32.const 0)))"
    )));
    assert_eq!(
        stopped(&end).1,
        "axl_step: ms must be between 1 and 1000, not 0."
    );
    let (end, _) = run(&module(&format!(
        "{init} (drop (call $step (i32.const 1001)))"
    )));
    assert_eq!(
        stopped(&end).1,
        "axl_step: ms must be between 1 and 1000, not 1001."
    );
    let (end, _) = run(&module(&format!(
        "{init} (drop (call $device (i32.const 70000) (i32.const 4)))"
    )));
    assert_eq!(*stopped(&end).0, StopKind::BadArgument);
    let (end, _) = run(&module(&format!(
        "{init} (call $imu_gyro (i32.const 65530))"
    )));
    assert!(stopped(&end).1.contains("outside the program's memory"));
    let (end, _) = run(&module(&format!(
        "{init} (call $motor_power (i32.const 1) (f64.const nan))"
    )));
    assert!(stopped(&end).1.contains("must be a number"));
    let (end, _) = run(&module(&format!(
        "{init} (call $motor_position (i32.const 1) (f64.const 1) (f64.const 0))"
    )));
    assert!(
        stopped(&end)
            .1
            .contains("max_rad_s must be greater than zero")
    );
}

#[test]
fn more_than_16_plot_channels_is_refused_by_the_robot() {
    // Channel names "plot" is one; use 17 distinct single-byte channels at addresses 400.. .
    let mut body = String::from("(drop (call $init (i32.const 1)))");
    for i in 0..17 {
        body.push_str(&format!("(i32.store8 (i32.const {}) (i32.const {})) (call $plot (i32.const {}) (i32.const 1) (f64.const 1))", 400 + i, 65 + i, 400 + i));
    }
    let (end, _) = run(&module(&body));
    assert_eq!(*stopped(&end).0, StopKind::Refused);
    assert!(stopped(&end).1.contains("16 plot channels"));
}

#[test]
fn mission_over_comes_from_step_and_main_returning_ends_the_program() {
    let body = "(drop (call $init (i32.const 1))) (loop $l (br_if $l (i32.eqz (call $step (i32.const 5)))))";
    let api = Recorder {
        over_after_steps: Some(3),
        ..Default::default()
    };
    let (end, c) = run_with(&module(body), &HostConfig::default(), api);
    assert_eq!(end, RunEnd::Finished);
    assert_eq!(c.api().steps, 3);
    assert!(c.api().ended);
    // After the mission is over, step keeps answering "over" without stepping the robot.
    let body = "(drop (call $init (i32.const 1))) (drop (call $step (i32.const 5))) (drop (call $step (i32.const 5))) (drop (call $step (i32.const 5))) (call $plot (i32.const 20) (i32.const 4) (f64.convert_i32_s (call $step (i32.const 5))))";
    let api = Recorder {
        over_after_steps: Some(1),
        ..Default::default()
    };
    let (_, c) = run_with(&module(body), &HostConfig::default(), api);
    assert_eq!(c.api().steps, 1);
    assert_eq!(c.api().plots[0].1, 1.0);
}

#[test]
fn an_endless_loop_hits_the_watchdog_within_two_and_a_half_seconds() {
    let body = "(drop (call $init (i32.const 1))) (loop $l (br $l))";
    let t = Instant::now();
    let (end, c) = run(&module(body));
    let took = t.elapsed();
    let (kind, msg) = stopped(&end);
    assert_eq!(*kind, StopKind::Watchdog);
    assert_eq!(
        msg,
        "Your program ran for 2 seconds without calling axl_step. A loop may be missing a step."
    );
    assert!(
        took >= Duration::from_millis(1900),
        "stopped too early: {took:?}"
    );
    assert!(
        took < Duration::from_millis(2500),
        "stopped too late: {took:?}"
    );
    assert!(c.api().ended);
}

#[test]
fn calling_step_keeps_the_watchdog_quiet() {
    // 30 iterations of a busy 150 ms-free loop with a step between: far under 2 s without a step.
    let cfg = HostConfig {
        tick: Duration::from_millis(5),
        watchdog_ticks: 8,
        ..Default::default()
    };
    let body = "(local $i i32) (drop (call $init (i32.const 1))) (loop $l (drop (call $step (i32.const 1))) (local.set $i (i32.add (local.get $i) (i32.const 1))) (br_if $l (i32.lt_u (local.get $i) (i32.const 200))))";
    let (end, c) = run_with(&module(body), &cfg, Recorder::default());
    assert_eq!(end, RunEnd::Finished);
    assert_eq!(c.api().steps, 200);
}

#[test]
fn wasi_stdout_and_stderr_go_to_the_log() {
    // fd_write(1, iov at 500 -> "hello\nwor" , ...) then fd_write(2, ...) and a partial line.
    let wat = r#"(module
      (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "hello\nwor")
      (data (i32.const 16) "ld")
      (data (i32.const 32) "oops\n")
      (func (export "_start")
        (i32.store (i32.const 500) (i32.const 0)) (i32.store (i32.const 504) (i32.const 9))
        (drop (call $fd_write (i32.const 1) (i32.const 500) (i32.const 1) (i32.const 600)))
        (i32.store (i32.const 500) (i32.const 16)) (i32.store (i32.const 504) (i32.const 2))
        (drop (call $fd_write (i32.const 1) (i32.const 500) (i32.const 1) (i32.const 600)))
        (i32.store (i32.const 500) (i32.const 32)) (i32.store (i32.const 504) (i32.const 5))
        (drop (call $fd_write (i32.const 2) (i32.const 500) (i32.const 1) (i32.const 600)))
        ;; an invalid descriptor is refused, not trapped
        (call_indirect (type 0) (i32.const 0))
      )
      (type (func))
      (table 1 funcref)
    )"#;
    // The last instruction traps (null table entry) on purpose; the log must survive it.
    let (end, c) = run_with(wat, &HostConfig::default(), Recorder::default());
    assert!(matches!(end, RunEnd::Stopped(_)));
    assert_eq!(
        c.api().logs,
        vec![
            (LogStream::Stdout, "hello".to_owned()),
            (LogStream::Stderr, "oops".to_owned()),
            (LogStream::Stdout, "world".to_owned()),
        ]
    );
}

#[test]
fn wasi_clocks_return_sim_time_and_random_is_seeded() {
    let wat = r#"(module
      (import "axl" "init" (func $init (param i32) (result i32)))
      (import "axl" "step" (func $step (param i32) (result i32)))
      (import "axl" "plot" (func $plot (param i32 i32 f64)))
      (import "wasi_snapshot_preview1" "clock_time_get" (func $clock (param i32 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "random_get" (func $random (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "t")
      (func (export "_start")
        (drop (call $init (i32.const 1)))
        (drop (call $step (i32.const 250)))
        (drop (call $clock (i32.const 1) (i64.const 0) (i32.const 64)))
        (call $plot (i32.const 0) (i32.const 1) (f64.convert_i64_u (i64.load (i32.const 64))))
        (drop (call $clock (i32.const 0) (i64.const 0) (i32.const 64)))
        (call $plot (i32.const 0) (i32.const 1) (f64.convert_i64_u (i64.load (i32.const 64))))
        (drop (call $random (i32.const 128) (i32.const 8)))
        (call $plot (i32.const 0) (i32.const 1) (f64.convert_i64_u (i64.load (i32.const 128))))
      )
    )"#;
    let get = |seed: u64| {
        let cfg = HostConfig {
            seed,
            ..Default::default()
        };
        let (end, c) = run_with(wat, &cfg, Recorder::default());
        assert_eq!(end, RunEnd::Finished);
        c.api().plots.iter().map(|p| p.1).collect::<Vec<_>>()
    };
    let a = get(1);
    // Sim time is 12.5 s (the recorder's offset) + 0.25 s, in nanoseconds, on both clocks.
    assert_eq!(a[0], 12.75e9);
    assert_eq!(a[1], 12.75e9);
    assert_eq!(get(1), a, "same seed, same bytes");
    assert_ne!(get(2)[2], a[2], "another seed, other bytes");
}

#[test]
fn exit_sleep_and_unknown_wasi_functions() {
    let exit = |code: u32| {
        format!(
            r#"(module
          (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
          (memory (export "memory") 1)
          (func (export "_start") (call $exit (i32.const {code}))))"#
        )
    };
    let (end, c) = run_with(&exit(0), &HostConfig::default(), Recorder::default());
    assert_eq!(end, RunEnd::Finished);
    assert!(c.api().ended);
    let (end, _) = run_with(&exit(3), &HostConfig::default(), Recorder::default());
    assert_eq!(*stopped(&end).0, StopKind::Exit);
    assert_eq!(stopped(&end).1, "Your program exited with code 3.");

    let sleep = r#"(module
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start") (drop (call $poll (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 0)))))"#;
    let (end, _) = run_with(sleep, &HostConfig::default(), Recorder::default());
    assert_eq!(*stopped(&end).0, StopKind::Unsupported);
    assert!(stopped(&end).1.contains("axl_step(ms)"));

    let unknown = r#"(module
      (import "wasi_snapshot_preview1" "sock_accept" (func $f (param i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start") (drop (call $f (i32.const 0) (i32.const 0) (i32.const 0)))))"#;
    let (end, _) = run_with(unknown, &HostConfig::default(), Recorder::default());
    assert_eq!(*stopped(&end).0, StopKind::Trap);
}

#[test]
fn traps_and_limits_are_explained() {
    let (end, _) = run(&module("(unreachable)"));
    assert_eq!(*stopped(&end).0, StopKind::Trap);
    assert!(stopped(&end).1.contains("panic"));
    let (end, _) = run(&module("(drop (i32.load (i32.const 70000)))"));
    assert!(stopped(&end).1.contains("outside its memory"));
    let rec = r#"(module (memory (export "memory") 1)
      (func $f (param i32) (result i32) (call $f (i32.add (local.get 0) (i32.const 1))))
      (func (export "_start") (drop (call $f (i32.const 0)))))"#;
    let (end, _) = run_with(rec, &HostConfig::default(), Recorder::default());
    assert!(stopped(&end).1.contains("stack"));
    let div = r#"(module (memory (export "memory") 1)
      (func (export "_start") (drop (i32.div_s (i32.const 1) (i32.load (i32.const 0))))))"#;
    let (end, _) = run_with(div, &HostConfig::default(), Recorder::default());
    assert!(stopped(&end).1.contains("divided"));
}

#[test]
fn load_errors_are_plain() {
    let cfg = HostConfig::default();
    let load = |wat: &str| Controller::new(wat.as_bytes(), Recorder::default(), &cfg).err();
    // More than 64 MB of memory at start.
    let big = r#"(module (memory (export "memory") 1100) (func (export "_start")))"#;
    match load(big) {
        Some(LoadError::Instantiate(m)) => assert!(m.contains("64 MB"), "{m}"),
        other => panic!("{other:?}"),
    }
    let unknown = r#"(module (import "axl" "teleport" (func)) (memory (export "memory") 1) (func (export "_start")))"#;
    assert!(matches!(
        load(unknown),
        Some(LoadError::UnknownImport { .. })
    ));
    let other = r#"(module (import "env" "f" (func)) (func (export "_start")))"#;
    assert!(matches!(load(other), Some(LoadError::UnknownImport { .. })));
    assert!(matches!(
        load("(module (memory (export \"memory\") 1))"),
        Some(LoadError::NoStart)
    ));
    assert!(matches!(load("garbage"), Some(LoadError::Invalid(_))));
}

#[test]
fn memory_cannot_grow_past_64_mb() {
    let wat = r#"(module
      (import "axl" "init" (func $init (param i32) (result i32)))
      (import "axl" "plot" (func $plot (param i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "g")
      (func (export "_start")
        (drop (call $init (i32.const 1)))
        ;; 1024 pages = 64 MiB total is the cap: growing by 1023 is allowed, by 1 more is not.
        (call $plot (i32.const 0) (i32.const 1) (f64.convert_i32_s (memory.grow (i32.const 1023))))
        (call $plot (i32.const 0) (i32.const 1) (f64.convert_i32_s (memory.grow (i32.const 1))))))"#;
    let (end, c) = run_with(wat, &HostConfig::default(), Recorder::default());
    assert_eq!(end, RunEnd::Finished);
    assert_eq!(
        c.api().plots[0].1,
        1.0,
        "grow returns the old size in pages"
    );
    assert_eq!(c.api().plots[1].1, -1.0, "the 1025th page is refused");
}

#[test]
fn the_abi_list_matches_the_module_the_tests_import() {
    for f in axl_host::abi::FUNCTIONS {
        assert!(
            IMPORTS.contains(&format!("\"axl\" \"{f}\"")),
            "{f} is not in the WAT imports"
        );
    }
    assert_eq!(axl_host::abi::FUNCTIONS.len(), 29);
}
