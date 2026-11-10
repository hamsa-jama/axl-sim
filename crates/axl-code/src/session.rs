//! A Code screen session: the toolchain status, the build log and the diagnostics of the last
//! build, and the build running in the background.

use std::path::PathBuf;

use axl_host::toolchain::{
    BuildEvent, BuildHandle, BuildOutcome, BuildRequest, Diagnostic, Lang, ToolchainStatus,
    spawn_build,
};

use crate::project::Project;

/// State of the build panel.
pub struct CodeSession {
    /// The open project.
    pub project: Project,
    /// What is installed (refresh with [`CodeSession::refresh_toolchain`]).
    pub toolchain: ToolchainStatus,
    /// The build log, one entry per line.
    pub log: Vec<String>,
    /// Diagnostics of the last build.
    pub diagnostics: Vec<Diagnostic>,
    /// The program built last, if it succeeded.
    pub wasm: Option<PathBuf>,
    /// Folder that holds `c/axl.h` and `rust/axl-robot`.
    pub sdk: PathBuf,
    build: Option<BuildHandle>,
}

impl CodeSession {
    /// A session for `project`.
    pub fn new(project: Project, sdk: PathBuf) -> Self {
        let toolchain_dir = project.dir.clone();
        Self {
            project,
            toolchain: ToolchainStatus::detect_in(Some(&toolchain_dir)),
            log: Vec::new(),
            diagnostics: Vec::new(),
            wasm: None,
            sdk,
            build: None,
        }
    }

    /// Look for the toolchain again.
    pub fn refresh_toolchain(&mut self) {
        self.toolchain = ToolchainStatus::detect_in(Some(&self.project.dir));
    }

    /// A build is running.
    pub fn building(&self) -> bool {
        self.build.is_some()
    }

    /// Start a build of the project. Does nothing while one runs; says what is missing when the
    /// toolchain is not ready.
    pub fn start_build(&mut self) {
        if self.build.is_some() {
            return;
        }
        self.log.clear();
        self.diagnostics.clear();
        self.wasm = None;
        let lang = self.project.lang;
        if !self.toolchain.ready(lang) {
            self.log.push(match lang {
                Lang::C => "The C toolchain is not installed yet.".to_owned(),
                Lang::Rust => "The Rust toolchain is not ready yet.".to_owned(),
            });
            self.log.extend(self.toolchain.install_steps(lang));
            return;
        }
        self.build = Some(spawn_build(BuildRequest {
            lang,
            project: self.project.dir.clone(),
            sdk: self.sdk.clone(),
            wasi_sdk: self.toolchain.wasi_sdk.clone(),
            env_remove: Vec::new(),
        }));
    }

    /// Take in what the build thread has sent. Returns the outcome when the build just finished.
    pub fn pump(&mut self) -> Option<BuildOutcome> {
        let handle = self.build.as_ref()?;
        let mut done = None;
        while let Ok(ev) = handle.events.try_recv() {
            match ev {
                BuildEvent::Line(l) => self.log.push(l),
                BuildEvent::Done(o) => done = Some(o),
            }
        }
        let o = done?;
        self.diagnostics.clone_from(&o.diagnostics);
        self.wasm.clone_from(&o.wasm);
        self.log.push(if o.success {
            "Build finished.".to_owned()
        } else {
            format!(
                "Build failed: {} error(s).",
                o.diagnostics
                    .iter()
                    .filter(|d| d.severity == axl_host::toolchain::Severity::Error)
                    .count()
            )
        });
        if let Some(h) = self.build.take() {
            h.join();
        }
        Some(o)
    }

    /// Switch language (opens or creates that project).
    pub fn switch_language(
        &mut self,
        lang: Lang,
        data_dir: &std::path::Path,
        templates: &std::path::Path,
    ) -> std::io::Result<()> {
        self.project = Project::open(data_dir, &self.project.mission, lang, templates, &self.sdk)?;
        self.log.clear();
        self.diagnostics.clear();
        self.wasm = None;
        self.refresh_toolchain();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn session(name: &str) -> (CodeSession, PathBuf) {
        let data = std::env::temp_dir().join(format!("axl-sess-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let p = Project::open(
            &data,
            "m01",
            Lang::C,
            &repo().join("templates"),
            &repo().join("sdk"),
        )
        .unwrap();
        (CodeSession::new(p, repo().join("sdk")), data)
    }

    fn wait(s: &mut CodeSession) -> BuildOutcome {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(120) {
            if let Some(o) = s.pump() {
                return o;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("build did not finish");
    }

    #[test]
    fn the_c_template_builds_and_an_error_gives_a_position() {
        let (mut s, data) = session("c");
        if !s.toolchain.c_ready() {
            assert!(
                std::env::var_os("AXL_REQUIRE_TOOLCHAINS").is_none(),
                "wasi-sdk is required here"
            );
            eprintln!("wasi-sdk not found: skipped");
            return;
        }
        s.start_build();
        assert!(s.building());
        let o = wait(&mut s);
        assert!(o.success, "{:?}", s.log);
        let wasm = s.wasm.clone().unwrap();
        assert!(wasm.is_file());
        assert!(s.log.iter().any(|l| l.starts_with("clang")));
        assert_eq!(s.log.last().unwrap(), "Build finished.");

        // A mistake on line 5 is reported with file, line and column.
        s.project
            .write("#include \"axl.h\"\nint main(void) {\n    axl_init(AXL_ABI_VERSION);\n    int a = 1;\n    axl_motor_powr(a, 0.5);\n    return 0;\n}\n")
            .unwrap();
        s.start_build();
        let o = wait(&mut s);
        assert!(!o.success);
        let d = s
            .diagnostics
            .iter()
            .find(|d| d.severity == axl_host::toolchain::Severity::Error)
            .expect("an error");
        assert_eq!((d.file.as_str(), d.line), ("main.c", 5), "{d:?}");
        assert!(d.column >= 5);
        assert!(s.wasm.is_none());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_missing_toolchain_prints_the_install_steps() {
        let (mut s, data) = session("missing");
        s.toolchain = ToolchainStatus::default();
        s.start_build();
        assert!(!s.building());
        assert!(s.log.iter().any(|l| l.contains("WASI_SDK_PATH")));
        assert!(s.log[0].contains("not installed"));
        let _ = std::fs::remove_dir_all(&data);
    }
}
