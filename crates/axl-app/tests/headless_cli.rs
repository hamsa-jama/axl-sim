//! End-to-end checks of the `axl` binary's headless mode.

use std::process::Command;

fn axl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_axl"))
}

#[test]
fn headless_version_prints_version() {
    let out = axl().args(["--headless", "--version"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.trim(), format!("axl {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn bare_headless_prints_version_and_succeeds() {
    let out = axl().arg("--headless").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().starts_with("axl "));
}

#[test]
fn a_robot_that_does_nothing_fails_with_exit_code_1_and_json() {
    let out = axl()
        .args(["--headless", "--mission", "m01", "--seed", "1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("\"mission\": \"m01\""), "{text}");
    assert!(text.contains("\"passed\": false"), "{text}");
}

#[test]
fn an_unknown_mission_is_a_setup_error_with_exit_code_2() {
    let out = axl()
        .args(["--headless", "--mission", "m99"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn mission_without_headless_is_a_usage_error() {
    let out = axl().args(["--mission", "m01"]).output().unwrap();
    assert!(!out.status.success());
}
