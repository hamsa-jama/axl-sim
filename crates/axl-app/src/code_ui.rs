//! The Code screen: an editor with line numbers and highlighting, the API reference parsed from
//! `axl.h`, the build log with clickable errors, and the toolbar (language, Build, Build and Run,
//! Open in my editor, toolchain chip). The logic lives in `axl-code`; this file is only egui.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axl_code::apiref::{self, Section};
use axl_code::highlight::{Kind, highlight};
use axl_code::project::{FileWatcher, Project};
use axl_code::session::CodeSession;
use axl_host::RunEnd;
use axl_host::toolchain::{Lang, Severity, ToolchainStatus};
use axl_render::bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::browser_ui::{LeftInset, TopInset};
use crate::sandbox::{CodeRobot, SandboxSim, run_program};

/// Everything the Code screen keeps.
#[derive(Resource)]
pub struct CodeScreen {
    session: CodeSession,
    watcher: Option<Mutex<FileWatcher>>,
    data_dir: PathBuf,
    templates: PathBuf,
    text: String,
    on_disk: String,
    conflict: bool,
    api: Vec<Section>,
    api_query: String,
    run_after_build: bool,
    program_log: Vec<String>,
    stop_at: Option<(String, u32)>,
    goto: Option<u32>,
    notice: String,
    target_install: Option<crossbeam_channel::Receiver<String>>,
    submit_after_build: bool,
    submission: Option<Vec<u8>>,
}

/// Find the folder that holds `templates/` and `sdk/`: `AXL_ROOT`, else the working directory
/// or the executable's folder or one of their parents.
pub fn find_root() -> Option<PathBuf> {
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Some(r) = std::env::var_os("AXL_ROOT") {
        starts.push(PathBuf::from(r));
    }
    if let Ok(c) = std::env::current_dir() {
        starts.push(c);
    }
    if let Ok(e) = std::env::current_exe()
        && let Some(d) = e.parent()
    {
        starts.push(d.to_path_buf());
    }
    starts
        .iter()
        .flat_map(|s| s.ancestors())
        .find(|d| d.join("templates/c").is_dir() && d.join("sdk/c/axl.h").is_file())
        .map(Path::to_path_buf)
}

impl CodeScreen {
    /// What the program and the build have said, oldest first.
    pub fn log_lines(&self) -> &[String] {
        &self.program_log
    }

    /// Open the screen for a mission.
    pub fn open(mission: &str, lang: Lang, run: bool) -> Result<Self, String> {
        let root =
            find_root().ok_or("could not find the templates and sdk folders (set AXL_ROOT)")?;
        let data_dir = axl_build::io::data_dir().ok_or("no data folder on this system")?;
        let templates = root.join("templates");
        let sdk = root.join("sdk");
        let project =
            Project::open(&data_dir, mission, lang, &templates, &sdk).map_err(|e| e.to_string())?;
        let text = project.read().map_err(|e| e.to_string())?;
        let watcher = FileWatcher::new(&project).ok().map(Mutex::new);
        let mut s = Self {
            session: CodeSession::new(project, sdk),
            watcher,
            data_dir,
            templates,
            on_disk: text.clone(),
            text,
            conflict: false,
            api: apiref::reference(),
            api_query: String::new(),
            run_after_build: run,
            program_log: Vec::new(),
            stop_at: None,
            goto: None,
            notice: String::new(),
            target_install: None,
            submit_after_build: false,
            submission: None,
        };
        if run {
            s.build(true);
        }
        Ok(s)
    }

    fn save(&mut self) {
        if self.text != self.on_disk {
            match self.session.project.write(&self.text) {
                Ok(()) => self.on_disk.clone_from(&self.text),
                Err(e) => self.notice = format!("Could not save: {e}"),
            }
        }
    }

    fn build(&mut self, run: bool) {
        self.save();
        self.run_after_build = run;
        self.program_log.clear();
        self.stop_at = None;
        self.session.start_build();
    }

    /// Build the program and hand it over for grading (see [`CodeScreen::take_submission`]).
    pub fn request_submit(&mut self) {
        self.build(false);
        self.submit_after_build = true;
        self.submission = None;
    }

    /// Is a build for grading under way?
    pub fn submit_pending(&self) -> bool {
        self.submit_after_build
    }

    /// The built program, once after a successful [`CodeScreen::request_submit`].
    pub fn take_submission(&mut self) -> Option<Vec<u8>> {
        self.submission.take()
    }

    fn reload(&mut self) {
        match self.session.project.read() {
            Ok(t) => {
                self.on_disk.clone_from(&t);
                self.text = t;
                self.conflict = false;
            }
            Err(e) => self.notice = format!("Could not read the file: {e}"),
        }
    }

    fn switch(&mut self, lang: Lang) {
        if lang == self.session.project.lang {
            return;
        }
        self.save();
        match self
            .session
            .switch_language(lang, &self.data_dir, &self.templates)
        {
            Ok(()) => {
                self.watcher = FileWatcher::new(&self.session.project).ok().map(Mutex::new);
                self.reload();
                self.program_log.clear();
                self.stop_at = None;
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
}

fn kind_color(k: Kind, dark: bool) -> egui::Color32 {
    let (d, l) = match k {
        Kind::Keyword => ((0xC5, 0x86, 0xC0), (0x8A, 0x2B, 0x8F)),
        Kind::Type => ((0x4E, 0xC9, 0xB0), (0x1B, 0x7A, 0x6B)),
        Kind::Axl => ((0x61, 0xAF, 0xEF), (0x15, 0x57, 0xB0)),
        Kind::Number => ((0xB5, 0xCE, 0xA8), (0x1F, 0x7A, 0x3A)),
        Kind::Str => ((0xCE, 0x91, 0x78), (0xA3, 0x3B, 0x1A)),
        Kind::Comment => ((0x6A, 0x99, 0x55), (0x5A, 0x7A, 0x4A)),
        Kind::Preproc => ((0x9C, 0x9C, 0x9C), (0x66, 0x66, 0x66)),
        Kind::Plain => ((0xDC, 0xDC, 0xDC), (0x20, 0x20, 0x20)),
    };
    let (r, g, b) = if dark { d } else { l };
    egui::Color32::from_rgb(r, g, b)
}

fn layout(ui: &egui::Ui, text: &str, lang: Lang, wrap: f32) -> std::sync::Arc<egui::Galley> {
    let dark = ui.visuals().dark_mode;
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    for s in highlight(text, lang) {
        job.append(
            &text[s.start..s.end],
            0.0,
            egui::TextFormat::simple(font.clone(), kind_color(s.kind, dark)),
        );
    }
    job.wrap.max_width = wrap;
    ui.painter().layout_job(job)
}

fn chip(ui: &mut egui::Ui, ok: bool, text: &str) -> egui::Response {
    let (fill, fg) = if ok {
        (
            egui::Color32::from_rgb(0x1E, 0x4D, 0x2B),
            egui::Color32::from_rgb(0xB8, 0xF0, 0xC8),
        )
    } else {
        (
            egui::Color32::from_rgb(0x6B, 0x2A, 0x1E),
            egui::Color32::from_rgb(0xFF, 0xD2, 0xC4),
        )
    };
    ui.add(egui::Button::new(egui::RichText::new(text).color(fg)).fill(fill))
}

/// Everything that is not drawing: watch the file, pump the build, start the program, read what
/// the program says.
pub fn code_tick(
    mut screen: ResMut<CodeScreen>,
    robot: Option<Res<CodeRobot>>,
    mut sim: Option<ResMut<SandboxSim>>,
    mut ins: ResMut<crate::sim_ui::Instruments>,
    mut live: ResMut<axl_render::instruments::LiveView>,
    mut trails: ResMut<axl_render::instruments::Trails>,
) {
    let s = &mut *screen;
    let changed = s
        .watcher
        .as_ref()
        .is_some_and(|w| w.lock().is_ok_and(|mut w| w.changed()));
    if changed {
        match s.session.project.read() {
            Ok(t) if t != s.on_disk => {
                if s.text == s.on_disk {
                    s.on_disk.clone_from(&t);
                    s.text = t;
                    s.notice = "Reloaded: the file was changed in another program.".into();
                } else {
                    s.conflict = true;
                }
            }
            _ => {}
        }
    }
    if let Some(rx) = &s.target_install {
        while let Ok(l) = rx.try_recv() {
            s.session.log.push(l);
        }
    }
    let built = s.session.pump();
    if let Some(o) = &built
        && s.submit_after_build
    {
        s.submit_after_build = false;
        match (o.success, o.wasm.as_ref().map(std::fs::read)) {
            (true, Some(Ok(bytes))) => s.submission = Some(bytes),
            _ => s.notice = "Fix the build errors, then check your solution again.".into(),
        }
    } else if let Some(o) = built
        && o.success
        && s.run_after_build
        && let (Some(r), Some(w)) = (robot.as_ref(), o.wasm.as_ref())
    {
        match std::fs::read(w)
            .map_err(|e| e.to_string())
            .and_then(|b| run_program(r, b))
        {
            Ok(new) => {
                if let Some(sim) = sim.as_mut() {
                    **sim = new;
                    ins.new_run();
                    trails.clear();
                    live.selected = None;
                    live.drag = None;
                }
            }
            Err(e) => s
                .program_log
                .push(format!("Could not start the program: {e}")),
        }
    }
    {
        for ev in std::mem::take(&mut ins.inbox) {
            match ev {
                axl_sim::SimEvent::Started => s.program_log.push("Program started.".into()),
                axl_sim::SimEvent::Log { stream, line } => {
                    let tag = match stream {
                        axl_host::LogStream::Stdout => "out",
                        axl_host::LogStream::Stderr => "err",
                    };
                    s.program_log.push(format!("[{tag}] {line}"));
                }
                axl_sim::SimEvent::Ended(Ok(RunEnd::Finished)) => s
                    .program_log
                    .push("Program finished. The robot has braked.".into()),
                axl_sim::SimEvent::Ended(Ok(RunEnd::Stopped(e))) => {
                    s.program_log.push(format!("Program stopped: {e}"));
                    s.stop_at = e.location.map(|l| (l.file, l.line));
                }
                axl_sim::SimEvent::Ended(Err(m)) => s.program_log.push(m),
                _ => {}
            }
        }
    }
}

/// All panels of the Code screen.
#[allow(clippy::too_many_arguments)]
pub fn code_panels(
    mut contexts: EguiContexts,
    mut screen: ResMut<CodeScreen>,
    mut inset: ResMut<LeftInset>,
    top: Res<TopInset>,
    view: Option<ResMut<axl_render::camera_rig::ViewInsets>>,
    robot: Option<Res<CodeRobot>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut root = egui::Ui::new(
        ctx.clone(),
        "code_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(egui::Rect::from_min_max(
                ctx.viewport_rect().min + egui::vec2(0.0, top.0.max(28.0)),
                ctx.viewport_rect().max - egui::vec2(0.0, 26.0),
            )),
    );
    let s = &mut *screen;

    // ---- API reference (right) ----
    egui::Panel::right("api_reference")
        .default_size(330.0)
        .show(&mut root, |ui| {
            ui.heading("API reference");
            ui.add(egui::TextEdit::singleline(&mut s.api_query).hint_text("Search functions"));
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("api_scroll")
                .show(ui, |ui| {
                    let q = s.api_query.clone();
                    let hits = apiref::search(&s.api, &q);
                    let mut last = "";
                    for (section, e) in hits {
                        if section != last {
                            ui.add_space(6.0);
                            ui.strong(section);
                            last = section;
                        }
                        egui::CollapsingHeader::new(egui::RichText::new(&e.name).monospace())
                            .id_salt(&e.name)
                            .default_open(!q.is_empty())
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(&e.signature).monospace().small());
                                ui.label(&e.doc);
                            });
                    }
                });
        });

    // ---- Editor, toolbar and log (left) ----
    let panel = egui::Panel::left("code_editor")
        .default_size(820.0)
        .min_size(420.0)
        .show(&mut root, |ui| {
            toolbar(ui, s, robot.as_ref().map(|r| r.seed));
            if s.conflict {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xE0, 0xA0, 0x40),
                        "The file was changed in another program.",
                    );
                    if ui.button("Load their version").clicked() {
                        s.reload();
                    }
                    if ui.button("Keep mine").clicked() {
                        s.conflict = false;
                        s.save();
                    }
                });
            }
            if !s.notice.is_empty() {
                ui.label(&s.notice);
            }
            ui.separator();
            let log_h = 170.0;
            let avail = ui.available_height() - log_h - 12.0;
            editor(ui, s, avail.max(120.0));
            ui.separator();
            log_view(ui, s);
        });
    inset.0 = panel.response.rect.width();
    let free = root.available_rect_before_wrap();
    if let Some(mut v) = view {
        v.rect = Some([free.min.x, free.min.y, free.max.x, free.max.y]);
    }
    Ok(())
}

fn toolbar(ui: &mut egui::Ui, s: &mut CodeScreen, robot_seed: Option<u64>) {
    let lang = s.session.project.lang;
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("Mission {}", s.session.project.mission));
        if let Some(seed) = robot_seed {
            ui.label(format!("Seed {seed}"))
                .on_hover_text("The world you see is the one grading uses for this seed");
        }
        ui.separator();
        for (l, name) in [(Lang::C, "C"), (Lang::Rust, "Rust")] {
            if ui.selectable_label(lang == l, name).clicked() {
                s.switch(l);
            }
        }
        ui.separator();
        let busy = s.session.building();
        if ui.add_enabled(!busy, egui::Button::new("Build")).clicked() {
            s.build(false);
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Build and Run"))
            .clicked()
        {
            s.build(true);
        }
        if ui.button("Open in my editor").clicked() {
            s.save();
            if let Err(e) = s.session.project.open_in_editor() {
                s.notice = format!("Could not open an editor: {e}");
            }
        }
        ui.separator();
        let ready = s.session.toolchain.ready(lang);
        let label = match (lang, ready) {
            (Lang::C, true) => "C toolchain ready".to_owned(),
            (Lang::Rust, true) => "Rust toolchain ready".to_owned(),
            (Lang::C, false) => "wasi-sdk missing".to_owned(),
            (Lang::Rust, false) => "Rust target missing".to_owned(),
        };
        let r = chip(ui, ready, &label);
        if !ready {
            r.on_hover_text(s.session.toolchain.install_steps(lang).join("\n"));
        }
        if ui.small_button("Refresh").clicked() {
            s.session.refresh_toolchain();
        }
        if lang == Lang::Rust
            && !s.session.toolchain.rust_target
            && s.session.toolchain.rustup
            && s.target_install.is_none()
            && ui.button("rustup target add wasm32-wasip1").clicked()
        {
            let (tx, rx) = crossbeam_channel::unbounded::<String>();
            s.target_install = Some(rx);
            s.session.log.push("rustup target add wasm32-wasip1".into());
            std::thread::spawn(move || {
                let mut cmd = ToolchainStatus::add_target_command();
                match cmd.output() {
                    Ok(o) => {
                        for l in String::from_utf8_lossy(&o.stderr)
                            .lines()
                            .chain(String::from_utf8_lossy(&o.stdout).lines())
                        {
                            let _ = tx.send(l.to_owned());
                        }
                        let _ = tx.send(if o.status.success() {
                            "Target installed. Press Refresh.".to_owned()
                        } else {
                            "The install failed. Check your internet connection and try the command in a terminal.".to_owned()
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(format!("Could not run rustup: {e}"));
                    }
                }
            });
        }
    });
}

fn editor(ui: &mut egui::Ui, s: &mut CodeScreen, height: f32) {
    let lang = s.session.project.lang;
    let goto = s.goto.take();
    let id = egui::Id::new("code_text");
    egui::ScrollArea::both()
        .id_salt("code_scroll")
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let lines = s.text.split('\n').count().max(1);
                let numbers: String = (1..=lines)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(numbers)
                            .monospace()
                            .color(ui.visuals().weak_text_color()),
                    )
                    .selectable(false),
                );
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    layout(ui, buf.as_str(), lang, wrap)
                };
                if let Some(line) = goto {
                    let idx: usize = s
                        .text
                        .split('\n')
                        .take(line.saturating_sub(1) as usize)
                        .map(|l| l.chars().count() + 1)
                        .sum();
                    if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(idx),
                        )));
                        egui::TextEdit::store_state(ui.ctx(), id, st);
                    }
                    ui.memory_mut(|m| m.request_focus(id));
                }
                let out = egui::TextEdit::multiline(&mut s.text)
                    .id(id)
                    .code_editor()
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .desired_rows(24)
                    .layouter(&mut layouter)
                    .show(ui);
                if goto.is_some() {
                    ui.scroll_to_rect(out.response.response.rect, Some(egui::Align::Center));
                    if let Some(c) = out.cursor_range {
                        let pos = out.galley.pos_from_cursor(c.primary);
                        let r = pos.translate(out.galley_pos.to_vec2());
                        ui.scroll_to_rect(
                            r.expand2(egui::vec2(0.0, 40.0)),
                            Some(egui::Align::Center),
                        );
                    }
                }
            });
        });
}

fn log_view(ui: &mut egui::Ui, s: &mut CodeScreen) {
    let mut jump: Option<u32> = None;
    ui.horizontal(|ui| {
        ui.strong("Build and run");
        if s.session.building() {
            ui.spinner();
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("log_scroll")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .max_height(160.0)
        .show(ui, |ui| {
            for d in &s.session.diagnostics {
                if d.severity == Severity::Note {
                    continue;
                }
                let color = if d.severity == Severity::Error {
                    egui::Color32::from_rgb(0xF0, 0x6A, 0x5A)
                } else {
                    egui::Color32::from_rgb(0xE0, 0xB0, 0x40)
                };
                let name = Path::new(&d.file)
                    .file_name()
                    .map_or_else(|| d.file.clone(), |f| f.to_string_lossy().into_owned());
                let text = format!("{}:{}:{}  {}", name, d.line, d.column, d.message);
                if ui
                    .link(egui::RichText::new(text).monospace().color(color))
                    .clicked()
                {
                    jump = Some(d.line);
                }
            }
            for l in &s.session.log {
                ui.label(egui::RichText::new(l).monospace().small());
            }
            for l in &s.program_log {
                ui.label(egui::RichText::new(l).monospace().small());
            }
            if let Some((file, line)) = &s.stop_at {
                let name = Path::new(file)
                    .file_name()
                    .map_or_else(|| file.clone(), |f| f.to_string_lossy().into_owned());
                if ui.link(format!("Go to {name}:{line}")).clicked() {
                    jump = Some(*line);
                }
            }
        });
    if jump.is_some() {
        s.goto = jump;
    }
}
