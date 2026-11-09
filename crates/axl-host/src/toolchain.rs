//! The toolchain check and the build pipeline for learner projects (SPEC "Building").
//!
//! C is built with the clang that comes with wasi-sdk; Rust with `cargo build --target
//! wasm32-wasip1 --release`. Both run as child processes on a background thread that streams
//! their output to the build log, and both kinds of compiler output are parsed into
//! file, line and column diagnostics for clickable markers.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crossbeam_channel::{Receiver, unbounded};

/// The wasi-sdk release the C templates are tested with (also pinned in `docs/toolchains.md`).
pub const WASI_SDK_VERSION: &str = "27";
/// The Rust target for controllers.
pub const RUST_TARGET: &str = "wasm32-wasip1";

/// The language of a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// C17 with wasi-sdk's clang.
    C,
    /// Rust with the `wasm32-wasip1` target.
    Rust,
}

impl Lang {
    /// Folder name under a mission's project folder.
    pub fn folder(self) -> &'static str {
        match self {
            Lang::C => "c",
            Lang::Rust => "rust",
        }
    }
}

/// What is installed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolchainStatus {
    /// `rustup` is on the path.
    pub rustup: bool,
    /// `cargo` is on the path.
    pub cargo: bool,
    /// The `wasm32-wasip1` target is installed for the active Rust.
    pub rust_target: bool,
    /// The wasi-sdk folder, if one was found.
    pub wasi_sdk: Option<PathBuf>,
}

impl ToolchainStatus {
    /// Look around: `rustup`, `cargo`, the target, and wasi-sdk via `WASI_SDK_PATH`, `/opt/wasi-sdk`,
    /// `~/wasi-sdk`, `C:\wasi-sdk` (and `third_party/wasi-sdk` next to the working directory, for
    /// development).
    pub fn detect() -> Self {
        let runs = |prog: &str| {
            Command::new(prog)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        let rust_target = Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .is_some_and(|root| {
                Path::new(&root)
                    .join("lib/rustlib")
                    .join(RUST_TARGET)
                    .is_dir()
            });
        Self {
            rustup: runs("rustup"),
            cargo: runs("cargo"),
            rust_target,
            wasi_sdk: find_wasi_sdk(),
        }
    }

    /// Rust projects can be built.
    pub fn rust_ready(&self) -> bool {
        self.cargo && self.rust_target
    }

    /// C projects can be built.
    pub fn c_ready(&self) -> bool {
        self.wasi_sdk.is_some()
    }

    /// Ready for this language.
    pub fn ready(&self, lang: Lang) -> bool {
        match lang {
            Lang::C => self.c_ready(),
            Lang::Rust => self.rust_ready(),
        }
    }

    /// The exact steps to install what is missing for `lang` (empty when ready).
    pub fn install_steps(&self, lang: Lang) -> Vec<String> {
        let mut v = Vec::new();
        match lang {
            Lang::Rust => {
                if !self.rustup && !self.cargo {
                    v.push(
                        "Install Rust from https://rustup.rs (it also installs rustup).".to_owned(),
                    );
                }
                if !self.rust_target {
                    v.push(format!("Run: rustup target add {RUST_TARGET}"));
                }
            }
            Lang::C => {
                if self.wasi_sdk.is_none() {
                    let (os, ext) = if cfg!(windows) {
                        ("windows", "tar.gz")
                    } else if cfg!(target_os = "macos") {
                        ("macos", "tar.gz")
                    } else {
                        ("linux", "tar.gz")
                    };
                    let arch = if cfg!(target_arch = "aarch64") {
                        "arm64"
                    } else {
                        "x86_64"
                    };
                    v.push(format!(
                        "Download wasi-sdk {WASI_SDK_VERSION} for your system (wasi-sdk-{WASI_SDK_VERSION}.0-{arch}-{os}.{ext}) from https://github.com/WebAssembly/wasi-sdk/releases/tag/wasi-sdk-{WASI_SDK_VERSION}"
                    ));
                    v.push(if cfg!(windows) {
                        "Unpack it to C:\\wasi-sdk (or anywhere, and set the WASI_SDK_PATH environment variable to that folder).".to_owned()
                    } else {
                        "Unpack it to /opt/wasi-sdk or ~/wasi-sdk (or anywhere, and set the WASI_SDK_PATH environment variable to that folder).".to_owned()
                    });
                    v.push(
                        "Restart AXL, or press the refresh button next to the toolchain chip."
                            .to_owned(),
                    );
                }
            }
        }
        v
    }

    /// The command behind the "install target" button: `rustup target add wasm32-wasip1`.
    pub fn add_target_command() -> Command {
        let mut c = Command::new("rustup");
        c.args(["target", "add", RUST_TARGET]);
        c
    }
}

fn is_wasi_sdk(p: &Path) -> bool {
    let clang = if cfg!(windows) {
        "bin/clang.exe"
    } else {
        "bin/clang"
    };
    p.join(clang).is_file() && p.join("share/wasi-sysroot").is_dir()
}

/// Find wasi-sdk (see [`ToolchainStatus::detect`]).
pub fn find_wasi_sdk() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("WASI_SDK_PATH") {
        candidates.push(PathBuf::from(p));
    }
    candidates.push(PathBuf::from("/opt/wasi-sdk"));
    if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        candidates.push(PathBuf::from(h).join("wasi-sdk"));
    }
    candidates.push(PathBuf::from("C:\\wasi-sdk"));
    if let Ok(cwd) = std::env::current_dir() {
        for d in cwd.ancestors() {
            candidates.push(d.join("third_party/wasi-sdk"));
        }
    }
    candidates.into_iter().find(|p| is_wasi_sdk(p))
}

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The build failed because of it.
    Error,
    /// Worth a look.
    Warning,
    /// Extra information for the previous message.
    Note,
}

/// A compiler message with its place in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// File as the compiler printed it.
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column (0 if unknown).
    pub column: u32,
    /// Severity.
    pub severity: Severity,
    /// The message.
    pub message: String,
}

/// Parse clang's `file:line:col: error: message` lines.
pub fn parse_clang_output(text: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for l in text.lines() {
        for (tag, severity) in [
            (": error: ", Severity::Error),
            (": fatal error: ", Severity::Error),
            (": warning: ", Severity::Warning),
            (": note: ", Severity::Note),
        ] {
            let Some(i) = l.find(tag) else { continue };
            // Split the position from the right so a drive letter (C:\...) survives.
            let mut parts = l[..i].rsplitn(3, ':');
            let (Some(col), Some(line), Some(file)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let (Ok(col), Ok(line)) = (col.parse(), line.parse()) {
                out.push(Diagnostic {
                    file: file.to_owned(),
                    line,
                    column: col,
                    severity,
                    message: l[i + tag.len()..].to_owned(),
                });
                break;
            }
        }
    }
    out
}

#[derive(serde::Deserialize)]
struct CargoLine {
    reason: Option<String>,
    message: Option<CargoMessage>,
    filenames: Option<Vec<String>>,
    target: Option<CargoTarget>,
}

#[derive(serde::Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
}

#[derive(serde::Deserialize)]
struct CargoMessage {
    level: String,
    message: String,
    rendered: Option<String>,
    spans: Vec<CargoSpan>,
}

#[derive(serde::Deserialize)]
struct CargoSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    is_primary: bool,
}

/// What one line of `cargo --message-format=json` holds: text for the log, a diagnostic, or the
/// built `.wasm`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CargoEvent {
    /// Text to show in the build log.
    pub text: Option<String>,
    /// A diagnostic.
    pub diagnostic: Option<Diagnostic>,
    /// The built binary.
    pub artifact: Option<PathBuf>,
}

/// Parse one line of cargo's JSON output.
pub fn parse_cargo_line(line: &str) -> CargoEvent {
    let Ok(l) = serde_json::from_str::<CargoLine>(line) else {
        return CargoEvent {
            text: Some(line.to_owned()),
            ..Default::default()
        };
    };
    match l.reason.as_deref() {
        Some("compiler-message") => {
            let Some(m) = l.message else {
                return CargoEvent::default();
            };
            let severity = match m.level.as_str() {
                "error" | "error: internal compiler error" => Severity::Error,
                "warning" => Severity::Warning,
                _ => Severity::Note,
            };
            let diagnostic = m.spans.iter().find(|s| s.is_primary).map(|s| Diagnostic {
                file: s.file_name.clone(),
                line: s.line_start,
                column: s.column_start,
                severity,
                message: m.message.clone(),
            });
            CargoEvent {
                text: m.rendered.or(Some(m.message)),
                diagnostic,
                artifact: None,
            }
        }
        Some("compiler-artifact") => {
            let is_bin = l.target.is_some_and(|t| t.kind.iter().any(|k| k == "bin"));
            let wasm = l
                .filenames
                .unwrap_or_default()
                .into_iter()
                .find(|f| f.ends_with(".wasm"));
            CargoEvent {
                artifact: if is_bin {
                    wasm.map(PathBuf::from)
                } else {
                    None
                },
                ..Default::default()
            }
        }
        _ => CargoEvent::default(),
    }
}

/// A build to run.
#[derive(Clone, Debug)]
pub struct BuildRequest {
    /// Language.
    pub lang: Lang,
    /// The project folder (`<data dir>/projects/<mission>/<c|rust>/`).
    pub project: PathBuf,
    /// The SDK folder that holds `c/axl.h`.
    pub sdk: PathBuf,
    /// wasi-sdk folder (C only); `None` finds it with [`find_wasi_sdk`].
    pub wasi_sdk: Option<PathBuf>,
}

/// The result of a build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOutcome {
    /// The compiler succeeded.
    pub success: bool,
    /// The built program.
    pub wasm: Option<PathBuf>,
    /// Errors and warnings with source positions.
    pub diagnostics: Vec<Diagnostic>,
}

/// What the background build thread sends.
#[derive(Debug)]
pub enum BuildEvent {
    /// One line for the build log.
    Line(String),
    /// The build is over.
    Done(BuildOutcome),
}

/// Run a build on the calling thread, sending each log line to `log`.
pub fn build(req: &BuildRequest, mut log: impl FnMut(&str)) -> BuildOutcome {
    let fail = |log: &mut dyn FnMut(&str), msg: &str| {
        log(msg);
        BuildOutcome {
            success: false,
            wasm: None,
            diagnostics: Vec::new(),
        }
    };
    match req.lang {
        Lang::C => {
            let Some(sdk) = req.wasi_sdk.clone().or_else(find_wasi_sdk) else {
                let steps = ToolchainStatus::default().install_steps(Lang::C).join("\n");
                return fail(&mut log, &format!("wasi-sdk was not found.\n{steps}"));
            };
            let mut sources: Vec<PathBuf> = std::fs::read_dir(&req.project)
                .map(|d| {
                    d.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e == "c"))
                        .collect()
                })
                .unwrap_or_default();
            sources.sort();
            if sources.is_empty() {
                return fail(&mut log, "There is no .c file in the project folder.");
            }
            let out = req.project.join("robot.wasm");
            let clang = sdk.join(if cfg!(windows) {
                "bin/clang.exe"
            } else {
                "bin/clang"
            });
            let mut cmd = Command::new(clang);
            cmd.arg(format!("--target={RUST_TARGET}"))
                .arg(format!(
                    "--sysroot={}",
                    sdk.join("share/wasi-sysroot").display()
                ))
                .args([
                    "-std=c17",
                    "-O2",
                    "-g",
                    "-Wall",
                    "-Wextra",
                    "-fno-color-diagnostics",
                ])
                .arg(format!("-I{}", req.sdk.join("c").display()))
                .args(sources.iter().map(|s| s.file_name().unwrap_or_default()))
                .arg("-o")
                .arg(&out)
                .current_dir(&req.project);
            log(&format!(
                "clang (wasi-sdk {WASI_SDK_VERSION}) {}",
                sources
                    .iter()
                    .filter_map(|s| s.file_name())
                    .map(|f| f.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
            let mut text = String::new();
            let ok = run_streaming(cmd, |l, _| {
                text.push_str(l);
                text.push('\n');
                log(l);
            });
            let diagnostics = parse_clang_output(&text);
            let success = ok && !diagnostics.iter().any(|d| d.severity == Severity::Error);
            BuildOutcome {
                success,
                wasm: success.then_some(out),
                diagnostics,
            }
        }
        Lang::Rust => {
            let mut cmd = Command::new("cargo");
            cmd.args([
                "build",
                "--target",
                RUST_TARGET,
                "--release",
                "--message-format=json",
            ])
            .current_dir(&req.project);
            log("cargo build --target wasm32-wasip1 --release");
            let (mut diagnostics, mut wasm) = (Vec::new(), None);
            let ok = run_streaming(cmd, |l, is_stdout| {
                if !is_stdout {
                    log(l);
                    return;
                }
                let ev = parse_cargo_line(l);
                if let Some(t) = ev.text {
                    for line in t.lines() {
                        log(line);
                    }
                }
                diagnostics.extend(ev.diagnostic);
                if ev.artifact.is_some() {
                    wasm = ev.artifact;
                }
            });
            let success = ok && wasm.is_some();
            BuildOutcome {
                success,
                wasm: if success { wasm } else { None },
                diagnostics,
            }
        }
    }
}

/// Run a command, calling `on_line(line, is_stdout)` for every output line. True on exit code 0.
fn run_streaming(mut cmd: Command, mut on_line: impl FnMut(&str, bool)) -> bool {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            on_line(&format!("Could not start the compiler: {e}"), false);
            return false;
        }
    };
    let (tx, rx) = unbounded::<(String, bool)>();
    let mut threads = Vec::new();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        threads.push(std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send((l, true));
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        threads.push(std::thread::spawn(move || {
            for l in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = tx.send((l, false));
            }
        }));
    }
    drop(tx);
    for (l, is_stdout) in rx {
        on_line(&l, is_stdout);
    }
    for t in threads {
        let _ = t.join();
    }
    child.wait().is_ok_and(|s| s.success())
}

/// A build running on its own thread.
pub struct BuildHandle {
    /// Log lines, then `Done`.
    pub events: Receiver<BuildEvent>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl BuildHandle {
    /// Wait for the build thread to end.
    pub fn join(mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Start a build on a background thread; its output streams through the handle.
pub fn spawn_build(req: BuildRequest) -> BuildHandle {
    let (tx, rx) = unbounded();
    let thread = std::thread::Builder::new()
        .name("axl-toolchain".into())
        .spawn(move || {
            let outcome = build(&req, |l| {
                let _ = tx.send(BuildEvent::Line(l.to_owned()));
            });
            let _ = tx.send(BuildEvent::Done(outcome));
        })
        .ok();
    BuildHandle { events: rx, thread }
}

/// Make `<data dir>/projects/<mission>/<c|rust>/` from `templates/<c|rust>/<mission>/` if it does
/// not exist yet. Existing files are never overwritten. In a Rust project the `axl-robot` path
/// dependency is pointed at the real SDK folder. Returns the project folder.
pub fn ensure_project(
    data_dir: &Path,
    mission: &str,
    lang: Lang,
    templates: &Path,
    sdk: &Path,
) -> std::io::Result<PathBuf> {
    let dest = data_dir.join("projects").join(mission).join(lang.folder());
    let src = templates.join(lang.folder()).join(mission);
    if !src.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "there is no template for mission {mission} in {}",
                lang.folder()
            ),
        ));
    }
    copy_missing(&src, &dest)?;
    if lang == Lang::Rust {
        let manifest = dest.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)?;
        let sdk_path = sdk.join("rust/axl-robot");
        let fixed = text.replace(
            "../../../sdk/rust/axl-robot",
            &sdk_path.to_string_lossy().replace('\\', "/"),
        );
        if fixed != text {
            std::fs::write(&manifest, fixed)?;
        }
    }
    Ok(dest)
}

fn copy_missing(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dest.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_missing(&e.path(), &to)?;
        } else if !to.exists() {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clang_lines_become_diagnostics() {
        let text = "main.c:12:5: error: use of undeclared identifier 'x'\n   12 |     x = 1;\n      |     ^\nmain.c:3:9: warning: unused variable 'y' [-Wunused-variable]\nC:\\p\\main.c:7:1: fatal error: 'axl.h' file not found\nmain.c:4:2: note: declared here\nnot a diagnostic: error: nope\n";
        let d = parse_clang_output(text);
        assert_eq!(d.len(), 4);
        assert_eq!(
            (d[0].file.as_str(), d[0].line, d[0].column, d[0].severity),
            ("main.c", 12, 5, Severity::Error)
        );
        assert_eq!(d[0].message, "use of undeclared identifier 'x'");
        assert_eq!(d[1].severity, Severity::Warning);
        assert_eq!((d[2].file.as_str(), d[2].line), ("C:\\p\\main.c", 7));
        assert_eq!(d[3].severity, Severity::Note);
    }

    #[test]
    fn cargo_json_lines_become_events() {
        let msg = r#"{"reason":"compiler-message","message":{"level":"error","message":"cannot find value `x`","rendered":"error: cannot find value `x`\n","spans":[{"file_name":"src/main.rs","line_start":9,"column_start":13,"is_primary":true},{"file_name":"src/lib.rs","line_start":1,"column_start":1,"is_primary":false}]}}"#;
        let e = parse_cargo_line(msg);
        let d = e.diagnostic.unwrap();
        assert_eq!(
            (d.file.as_str(), d.line, d.column, d.severity),
            ("src/main.rs", 9, 13, Severity::Error)
        );
        assert!(e.text.unwrap().starts_with("error: cannot find value"));
        let art = r#"{"reason":"compiler-artifact","target":{"kind":["bin"]},"filenames":["/p/target/wasm32-wasip1/release/robot.wasm"]}"#;
        assert_eq!(
            parse_cargo_line(art).artifact,
            Some(PathBuf::from("/p/target/wasm32-wasip1/release/robot.wasm"))
        );
        let lib = r#"{"reason":"compiler-artifact","target":{"kind":["lib"]},"filenames":["/p/liba.rlib"]}"#;
        assert_eq!(parse_cargo_line(lib).artifact, None);
        assert_eq!(
            parse_cargo_line("plain text").text.as_deref(),
            Some("plain text")
        );
        assert_eq!(
            parse_cargo_line(r#"{"reason":"build-finished","success":true}"#),
            CargoEvent::default()
        );
    }

    #[test]
    fn install_steps_name_exact_commands() {
        let none = ToolchainStatus::default();
        assert!(
            none.install_steps(Lang::Rust)
                .iter()
                .any(|s| s == "Run: rustup target add wasm32-wasip1")
        );
        assert!(none.install_steps(Lang::Rust)[0].contains("rustup.rs"));
        let c = none.install_steps(Lang::C);
        assert!(c[0].contains("wasi-sdk-27") && c[0].contains("github.com/WebAssembly/wasi-sdk"));
        assert!(c.iter().any(|s| s.contains("WASI_SDK_PATH")));
        let ready = ToolchainStatus {
            rustup: true,
            cargo: true,
            rust_target: true,
            wasi_sdk: Some(PathBuf::from("/x")),
        };
        assert!(
            ready.install_steps(Lang::Rust).is_empty() && ready.install_steps(Lang::C).is_empty()
        );
        assert!(ready.ready(Lang::C) && ready.ready(Lang::Rust) && !none.ready(Lang::C));
        assert_eq!(
            format!("{:?}", ToolchainStatus::add_target_command()),
            "\"rustup\" \"target\" \"add\" \"wasm32-wasip1\""
        );
    }

    #[test]
    fn projects_are_copied_once_and_never_overwritten() {
        let tmp = std::env::temp_dir().join(format!("axl-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let templates = tmp.join("templates");
        std::fs::create_dir_all(templates.join("rust/m01/src")).unwrap();
        std::fs::write(
            templates.join("rust/m01/Cargo.toml"),
            "axl-robot = { path = \"../../../sdk/rust/axl-robot\" }",
        )
        .unwrap();
        std::fs::write(templates.join("rust/m01/src/main.rs"), "fn main() {}").unwrap();
        let data = tmp.join("data");
        let sdk = tmp.join("sdk");
        let p = ensure_project(&data, "m01", Lang::Rust, &templates, &sdk).unwrap();
        assert_eq!(p, data.join("projects/m01/rust"));
        let manifest = std::fs::read_to_string(p.join("Cargo.toml")).unwrap();
        assert!(
            manifest.contains(
                &sdk.join("rust/axl-robot")
                    .to_string_lossy()
                    .replace('\\', "/")
            )
        );
        std::fs::write(p.join("src/main.rs"), "// mine").unwrap();
        ensure_project(&data, "m01", Lang::Rust, &templates, &sdk).unwrap();
        assert_eq!(
            std::fs::read_to_string(p.join("src/main.rs")).unwrap(),
            "// mine"
        );
        assert!(ensure_project(&data, "m99", Lang::C, &templates, &sdk).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
