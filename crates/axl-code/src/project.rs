//! Project folders (`<data dir>/projects/<mission>/<c|rust>/`) and watching them for edits made
//! in the learner's own editor.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use axl_host::toolchain::{Lang, ensure_project};
use crossbeam_channel::{Receiver, unbounded};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// A learner's project for one mission in one language.
#[derive(Clone, Debug)]
pub struct Project {
    /// Mission id, for example `m01`.
    pub mission: String,
    /// Language.
    pub lang: Lang,
    /// The project folder.
    pub dir: PathBuf,
}

impl Project {
    /// Open the project, making it from the template on first use (files that exist are never
    /// overwritten).
    pub fn open(
        data_dir: &Path,
        mission: &str,
        lang: Lang,
        templates: &Path,
        sdk: &Path,
    ) -> std::io::Result<Self> {
        let dir = ensure_project(data_dir, mission, lang, templates, sdk)?;
        Ok(Self {
            mission: mission.to_owned(),
            lang,
            dir,
        })
    }

    /// The file the editor shows.
    pub fn main_file(&self) -> PathBuf {
        match self.lang {
            Lang::C => self.dir.join("main.c"),
            Lang::Rust => self.dir.join("src").join("main.rs"),
        }
    }

    /// Read the main file.
    pub fn read(&self) -> std::io::Result<String> {
        std::fs::read_to_string(self.main_file())
    }

    /// Write the main file.
    pub fn write(&self, text: &str) -> std::io::Result<()> {
        std::fs::write(self.main_file(), text)
    }

    /// Open the project folder in the system's file manager or editor association.
    pub fn open_in_editor(&self) -> std::io::Result<()> {
        open_path(&self.main_file())
    }
}

/// Ask the operating system to open a file with the user's own program.
pub fn open_path(path: &Path) -> std::io::Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        Command::new("xdg-open")
    };
    cmd.arg(path).spawn().map(|_| ())
}

/// Watches a project folder. [`FileWatcher::changed`] says when the main file was edited from
/// outside, once edits have been quiet for a moment.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<PathBuf>,
    pending: Option<Instant>,
    main: PathBuf,
}

/// How long edits must be quiet before they count.
pub const QUIET: Duration = Duration::from_millis(150);

impl FileWatcher {
    /// Start watching `project`.
    pub fn new(project: &Project) -> notify::Result<Self> {
        let (tx, events) = unbounded();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res
                    && !matches!(ev.kind, notify::EventKind::Access(_))
                {
                    for p in ev.paths {
                        let _ = tx.send(p);
                    }
                }
            })?;
        watcher.watch(&project.dir, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            events,
            pending: None,
            main: project.main_file(),
        })
    }

    /// True once, after the main file has changed and then stayed quiet for [`QUIET`].
    pub fn changed(&mut self) -> bool {
        while let Ok(p) = self.events.try_recv() {
            if same_file(&p, &self.main) {
                self.pending = Some(Instant::now());
            }
        }
        match self.pending {
            Some(t) if t.elapsed() >= QUIET => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            a.file_name() == b.file_name()
                && a.parent().map(Path::file_name) == b.parent().map(Path::file_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("axl-code-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn a_project_is_copied_from_the_template_once() {
        let data = temp("proj");
        let p = Project::open(
            &data,
            "m01",
            Lang::C,
            &repo().join("templates"),
            &repo().join("sdk"),
        )
        .unwrap();
        assert_eq!(p.dir, data.join("projects/m01/c"));
        assert!(p.read().unwrap().contains("axl_init(AXL_ABI_VERSION)"));
        p.write("// mine\n").unwrap();
        let again = Project::open(
            &data,
            "m01",
            Lang::C,
            &repo().join("templates"),
            &repo().join("sdk"),
        )
        .unwrap();
        assert_eq!(again.read().unwrap(), "// mine\n");
        let r = Project::open(
            &data,
            "m01",
            Lang::Rust,
            &repo().join("templates"),
            &repo().join("sdk"),
        )
        .unwrap();
        assert!(r.main_file().ends_with("src/main.rs"));
        assert!(r.read().unwrap().contains("fn main"));
        let manifest = std::fs::read_to_string(r.dir.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("sdk/rust/axl-robot"), "{manifest}");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn an_outside_edit_is_noticed_once_after_it_goes_quiet() {
        let data = temp("watch");
        let p = Project::open(
            &data,
            "m01",
            Lang::C,
            &repo().join("templates"),
            &repo().join("sdk"),
        )
        .unwrap();
        let mut w = FileWatcher::new(&p).unwrap();
        assert!(!w.changed());
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(p.main_file(), "// edited elsewhere\n").unwrap();
        let t = Instant::now();
        let mut seen = false;
        while t.elapsed() < Duration::from_secs(5) {
            if w.changed() {
                seen = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(seen, "the edit was not noticed");
        std::thread::sleep(Duration::from_millis(400));
        assert!(!w.changed(), "reported only once");
        let _ = std::fs::remove_dir_all(&data);
    }
}
