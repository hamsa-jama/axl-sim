//! `cargo xtask sdk-check`: build every template and reference solution, then grade them with the
//! headless runner. A template must build and must not pass its mission (it does nothing); a
//! solution must build and pass on every grading seed.
//!
//! C needs wasi-sdk. Rust needs the `wasm32-wasip1` target; without it the Rust projects only get
//! `cargo check` on the host (CI passes `--require-rust` so that this cannot hide a failure).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::mujoco_env::mujoco_env;

const MISSIONS: [&str; 12] = [
    "m01", "m02", "m03", "m04", "m05", "m06", "m07", "m08", "m09", "m10", "m11", "m12",
];
const WASM_TARGET: &str = "wasm32-wasip1";

/// What to do about a missing toolchain.
#[derive(Clone, Copy)]
pub struct Options {
    /// Fail instead of skipping when wasi-sdk is missing.
    pub require_c: bool,
    /// Fail instead of falling back to `cargo check` when the wasm target is missing.
    pub require_rust: bool,
}

struct Ctx {
    root: PathBuf,
    out: PathBuf,
    axl: PathBuf,
    env: Vec<(String, String)>,
    failures: Vec<String>,
    graded: usize,
}

pub fn run(root: &Path, opts: Options) -> Result<()> {
    let env = mujoco_env(root)?;
    let out = root.join("target/sdk-check");
    std::fs::create_dir_all(&out)?;

    println!("building the headless axl binary");
    let mut build = Command::new(cargo());
    build
        .current_dir(root)
        .args(["build", "--package", "axl-app", "--no-default-features"]);
    for (k, v) in &env {
        build.env(k, v);
    }
    if !build.status()?.success() {
        bail!("could not build axl");
    }
    let mut ctx = Ctx {
        root: root.to_owned(),
        out,
        axl: root.join("target/debug").join(exe("axl")),
        env,
        failures: Vec::new(),
        graded: 0,
    };

    match find_wasi_sdk(root) {
        Some(sdk) => check_c(&mut ctx, &sdk),
        None if opts.require_c => ctx.failures.push("wasi-sdk was not found".into()),
        None => println!("SKIP C: wasi-sdk was not found (set WASI_SDK_PATH)"),
    }
    if wasm_target_installed() {
        check_rust_wasm(&mut ctx);
    } else if opts.require_rust {
        ctx.failures
            .push(format!("the {WASM_TARGET} target is not installed"));
    } else {
        println!(
            "SKIP Rust grading: the {WASM_TARGET} target is not installed; host cargo check only"
        );
        check_rust_host(&mut ctx);
    }

    println!(
        "\nsdk-check: {} controllers graded, {} problems",
        ctx.graded,
        ctx.failures.len()
    );
    for f in &ctx.failures {
        println!("  FAIL {f}");
    }
    if ctx.failures.is_empty() {
        Ok(())
    } else {
        bail!("sdk-check failed")
    }
}

fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn find_wasi_sdk(root: &Path) -> Option<PathBuf> {
    let clang = exe("bin/clang");
    let mut candidates: Vec<PathBuf> = std::env::var_os("WASI_SDK_PATH")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    candidates.push(root.join("third_party/wasi-sdk"));
    if let Ok(dir) = std::fs::read_dir(root.join("third_party")) {
        let mut found: Vec<PathBuf> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("wasi-sdk"))
            })
            .collect();
        found.sort();
        candidates.extend(found);
    }
    candidates.into_iter().find(|p| p.join(&clang).is_file())
}

fn wasm_target_installed() -> bool {
    let Ok(out) = Command::new("rustc").args(["--print", "sysroot"]).output() else {
        return false;
    };
    let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    Path::new(&sysroot)
        .join("lib/rustlib")
        .join(WASM_TARGET)
        .is_dir()
}

/// Run `axl --headless` on a controller over all grading seeds. `true` when it passed.
fn grade(ctx: &mut Ctx, mission: &str, wasm: &Path) -> bool {
    let mut cmd = Command::new(&ctx.axl);
    cmd.current_dir(&ctx.root)
        .args(["--headless", "--mission", mission, "--all-seeds"])
        .arg("--controller")
        .arg(wasm);
    if mission == "m12" {
        cmd.arg("--robot")
            .arg(ctx.root.join("missions/solutions/m12.axlbot"));
    }
    for (k, v) in &ctx.env {
        cmd.env(k, v);
    }
    ctx.graded += 1;
    cmd.output()
        .map(|o| o.status.code() == Some(0))
        .unwrap_or(false)
}

/// Grade one built controller. Solutions must pass; templates must not.
fn expect(ctx: &mut Ctx, label: &str, mission: &str, wasm: &Path, should_pass: bool) {
    let passed = grade(ctx, mission, wasm);
    let ok = passed == should_pass;
    println!(
        "{} {label}: {}",
        if ok { "ok  " } else { "FAIL" },
        if passed { "passes" } else { "does not pass" }
    );
    if !ok {
        ctx.failures.push(format!(
            "{label} {}",
            if should_pass {
                "does not pass its mission"
            } else {
                "passes its mission without doing anything"
            }
        ));
    }
}

fn check_c(ctx: &mut Ctx, sdk: &Path) {
    for m in MISSIONS {
        for (kind, dir, should_pass) in [
            ("template", ctx.root.join("templates/c").join(m), false),
            (
                "solution",
                ctx.root.join("missions/solutions/c").join(m),
                true,
            ),
        ] {
            let label = format!("C {kind} {m}");
            let wasm = ctx.out.join(format!("{m}-{kind}-c.wasm"));
            let mut clang = Command::new(sdk.join(exe("bin/clang")));
            clang
                .arg(format!("--target={WASM_TARGET}"))
                .arg(format!(
                    "--sysroot={}",
                    sdk.join("share/wasi-sysroot").display()
                ))
                .args(["-std=c17", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(format!("-I{}", ctx.root.join("sdk/c").display()))
                .arg(dir.join("main.c"))
                .arg("-o")
                .arg(&wasm);
            if !clang.status().is_ok_and(|s| s.success()) {
                println!("FAIL {label}: does not build");
                ctx.failures.push(format!("{label} does not build"));
                continue;
            }
            expect(ctx, &label, m, &wasm, should_pass);
        }
    }
}

fn run_cargo(dir: &Path, args: &[&str], target_dir: &Path) -> Result<bool> {
    let status = Command::new(cargo())
        .current_dir(dir)
        .args(args)
        .env("CARGO_TARGET_DIR", target_dir)
        .status()
        .with_context(|| format!("running cargo in {}", dir.display()))?;
    Ok(status.success())
}

fn check_rust_wasm(ctx: &mut Ctx) {
    let target_dir = ctx.out.join("rust-target");
    let release = target_dir.join(WASM_TARGET).join("release");
    // Templates: one project each, the crate is called `robot`.
    for m in MISSIONS {
        let label = format!("Rust template {m}");
        let dir = ctx.root.join("templates/rust").join(m);
        let built = run_cargo(
            &dir,
            &["build", "--release", "--target", WASM_TARGET],
            &target_dir,
        )
        .unwrap_or(false);
        if !built {
            ctx.failures.push(format!("{label} does not build"));
            continue;
        }
        let wasm = ctx.out.join(format!("{m}-template-rust.wasm"));
        if std::fs::copy(release.join("robot.wasm"), &wasm).is_err() {
            ctx.failures.push(format!("{label} produced no robot.wasm"));
            continue;
        }
        expect(ctx, &label, m, &wasm, false);
    }
    // Solutions: one workspace.
    let dir = ctx.root.join("missions/solutions/rust");
    let sol_target = ctx.out.join("rust-solutions-target");
    let built = run_cargo(
        &dir,
        &["build", "--release", "--workspace", "--target", WASM_TARGET],
        &sol_target,
    )
    .unwrap_or(false);
    if !built {
        ctx.failures.push("Rust solutions do not build".to_owned());
        return;
    }
    for m in MISSIONS {
        let wasm = sol_target
            .join(WASM_TARGET)
            .join("release")
            .join(format!("{m}.wasm"));
        expect(ctx, &format!("Rust solution {m}"), m, &wasm, true);
    }
}

fn check_rust_host(ctx: &mut Ctx) {
    let target_dir = ctx.out.join("rust-host-target");
    let dir = ctx.root.join("missions/solutions/rust");
    if !run_cargo(&dir, &["check", "--workspace"], &target_dir).unwrap_or(false) {
        ctx.failures.push("Rust solutions do not check".to_owned());
    }
    for m in MISSIONS {
        let dir = ctx.root.join("templates/rust").join(m);
        if !run_cargo(&dir, &["check"], &target_dir).unwrap_or(false) {
            ctx.failures
                .push(format!("Rust template {m} does not check"));
        }
    }
}
