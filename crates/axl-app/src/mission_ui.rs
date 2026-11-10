//! The mission panel next to the Code screen: the brief, the warm-up questions, the prediction
//! asked before the first run, the three hints, and the Results screen (pass or fail per
//! criterion, metrics, stars, a replay of the recorded path and one hint). The logic is in
//! `axl-missions` (`learn`, `progress`, `results`); this file is egui and a grading thread.

use std::path::PathBuf;

use axl_missions::def::MissionDef;
use axl_missions::learn::warmup::{self, Question};
use axl_missions::library::{builtin_missions, mission};
use axl_missions::progress::{Progress, day_of};
use axl_missions::results::{self, Block};
use axl_parts::catalog::Catalog;
use axl_render::bevy::prelude::*;
use axl_sim::mission::{GradedRun, grade_mission, robot_assembly};
use bevy_egui::{EguiContexts, egui};
use egui_plot::{Line, Plot, PlotPoints, Points};

use crate::browser_ui::{LeftInset, TopInset};
use crate::code_ui::CodeScreen;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Brief,
    Results,
}

struct WarmUp {
    question: Question,
    input: String,
    correct: Option<bool>,
}

type GradeMsg = Result<Box<GradedRun>, String>;

/// Everything the mission panel keeps.
#[derive(Resource)]
pub struct MissionScreen {
    def: MissionDef,
    progress: Progress,
    progress_path: Option<PathBuf>,
    day: f64,
    warmups: Vec<WarmUp>,
    predict_input: String,
    tab: Tab,
    grading: Option<crossbeam_channel::Receiver<GradeMsg>>,
    result: Option<Box<GradedRun>>,
    replay_seed: usize,
    replay_t: f64,
    notice: String,
    /// The program that was graded last, to watch it again in 3D.
    last_wasm: Option<Vec<u8>>,
    /// Grading just finished: the app moves to the Results screen.
    just_graded: bool,
    /// The learner asked to watch this seed in 3D.
    watch: Option<u64>,
}

/// The newest `.axlbot` in the robots folder: the design a build mission (m12) is graded with.
pub fn newest_design() -> Option<PathBuf> {
    let dir = axl_build::io::robots_dir()?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "axlbot"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max()
        .map(|(_, p)| p)
}

impl MissionScreen {
    /// Open the panel for a mission, loading the saved progress.
    pub fn open(id: &str) -> Result<Self, String> {
        let def = mission(id).map_err(|e| e.to_string())?;
        let progress_path = axl_build::io::data_dir().map(|d| d.join("progress.ron"));
        let progress = progress_path
            .as_deref()
            .map(Progress::load)
            .unwrap_or_default();
        let day = day_of(std::time::SystemTime::now());
        // The ideas of the earlier missions whose memory cards are due.
        let earlier: Vec<String> = builtin_missions()
            .map(|all| {
                all.iter()
                    .take_while(|m| m.id != def.id)
                    .filter_map(|m| m.concepts.first().cloned())
                    .collect()
            })
            .unwrap_or_default();
        let warmups = warmup::pick_due(&progress.cards, &earlier, day, 3)
            .iter()
            .enumerate()
            .filter_map(|(i, c)| warmup::generate(c, day as u64 * 7 + i as u64))
            .map(|question| WarmUp {
                question,
                input: String::new(),
                correct: None,
            })
            .collect();
        Ok(Self {
            def,
            progress,
            progress_path,
            day,
            warmups,
            predict_input: String::new(),
            tab: Tab::Brief,
            grading: None,
            result: None,
            replay_seed: 0,
            replay_t: 0.0,
            notice: String::new(),
            last_wasm: None,
            just_graded: false,
            watch: None,
        })
    }

    /// The mission.
    pub fn def(&self) -> &MissionDef {
        &self.def
    }

    /// Remember this as the mission Home's Continue goes back to.
    pub fn mark_opened(&mut self) {
        self.progress.set_last(&self.def.id);
        self.save();
    }

    /// True once after grading finishes.
    pub fn take_just_graded(&mut self) -> bool {
        std::mem::take(&mut self.just_graded)
    }

    /// The seed the learner asked to watch in 3D, with the program that was graded.
    pub fn take_watch(&mut self) -> Option<(u64, Vec<u8>)> {
        let seed = self.watch.take()?;
        Some((seed, self.last_wasm.clone()?))
    }

    /// Say something on the mission panel.
    pub fn say(&mut self, text: impl Into<String>) {
        self.notice = text.into();
    }

    fn save(&mut self) {
        if let Some(p) = &self.progress_path
            && let Err(e) = self.progress.save(p)
        {
            self.notice = format!("Could not save your progress: {e}");
        }
    }

    fn predicted(&self) -> Option<f64> {
        self.progress
            .missions
            .get(&self.def.id)
            .and_then(|p| p.predicted)
    }

    /// May the learner submit? Not before the prediction (when the mission asks for one).
    fn needs_prediction(&self) -> bool {
        self.def.predict.is_some() && self.predicted().is_none()
    }

    /// Check a built program on every grading seed (the `--grade` flag).
    pub fn grade_program(&mut self, wasm: Vec<u8>) {
        self.start_grading(wasm);
    }

    fn start_grading(&mut self, wasm: Vec<u8>) {
        let def = self.def.clone();
        let hints_used = self.progress.hints_open(&def.id) > 0;
        let design = newest_design();
        self.last_wasm = Some(wasm.clone());
        let (tx, rx) = crossbeam_channel::unbounded::<GradeMsg>();
        self.grading = Some(rx);
        std::thread::spawn(move || {
            let run = || -> GradeMsg {
                let cat = Catalog::builtin();
                let asm =
                    robot_assembly(&def, &cat, design.as_deref()).map_err(|e| e.to_string())?;
                grade_mission(
                    &def,
                    &asm,
                    &cat,
                    Some(&wasm),
                    &def.grading_seeds,
                    hints_used,
                )
                .map(Box::new)
                .map_err(|e| e.to_string())
            };
            let _ = tx.send(run());
        });
    }

    fn finish_grading(&mut self, run: Box<GradedRun>) {
        let first = self.def.concepts.first().cloned();
        let was_failing = !self.progress.hints_unlocked(&self.def.id);
        self.progress.record_run(
            &self.def.id,
            run.grade.passed,
            run.grade.stars,
            self.progress.hints_open(&self.def.id) > 0,
            first.as_deref(),
            self.day,
        );
        self.save();
        self.replay_seed = 0;
        self.replay_t = run
            .runs
            .first()
            .and_then(|r| r.trace.last())
            .map_or(0.0, |s| s.t);
        self.result = Some(run);
        self.just_graded = true;
        if was_failing && self.progress.hints_unlocked(&self.def.id) {
            self.notice = "Hints are unlocked now: open one when you are ready.".into();
        }
    }
}

/// Hand a built program to the grader and take in its verdict.
pub fn mission_tick(mut m: ResMut<MissionScreen>, mut code: ResMut<CodeScreen>) {
    if let Some(bytes) = code.take_submission() {
        m.start_grading(bytes);
    }
    let msg = m.grading.as_ref().and_then(|rx| rx.try_recv().ok());
    if let Some(msg) = msg {
        m.grading = None;
        match msg {
            Ok(run) => m.finish_grading(run),
            Err(e) => m.notice = format!("The mission could not be run: {e}"),
        }
    }
}

fn stars_text(n: u8) -> String {
    (0..3)
        .map(|i| if i < n { '\u{2605}' } else { '\u{2606}' })
        .collect()
}

/// The panel.
pub fn mission_panel(
    mut contexts: EguiContexts,
    mut m: ResMut<MissionScreen>,
    mut code: ResMut<CodeScreen>,
    inset: Res<LeftInset>,
    top: Res<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let screen = ctx.viewport_rect();
    let busy = m.grading.is_some() || code.submit_pending();
    let m = &mut *m;
    egui::Window::new(format!("Mission {}: {}", m.def.id, m.def.title))
        .default_pos(egui::pos2(inset.0 + 12.0, top.0.max(28.0) + 12.0))
        .default_size(egui::vec2(
            (screen.width() - inset.0 - 360.0).clamp(300.0, 460.0),
            540.0,
        ))
        .constrain_to(screen)
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut m.tab, Tab::Brief, "Brief");
                ui.add_enabled_ui(m.result.is_some(), |ui| {
                    ui.selectable_value(&mut m.tab, Tab::Results, "Results");
                });
                ui.separator();
                let stars = m.progress.stars(&m.def.id);
                ui.label(format!("Best: {}", stars_text(stars)));
            });
            ui.separator();
            if !m.notice.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0xA0, 0x40), &m.notice);
            }
            egui::ScrollArea::vertical()
                .id_salt("mission_scroll")
                .max_height(screen.height() - 220.0)
                .show(ui, |ui| match m.tab {
                    Tab::Brief => brief_tab(ui, m, &mut code, busy),
                    Tab::Results => results_tab(ui, m, &mut code, busy),
                });
        });
    Ok(())
}

fn submit_button(ui: &mut egui::Ui, m: &MissionScreen, code: &mut CodeScreen, busy: bool) {
    ui.horizontal(|ui| {
        let ok = !busy && !m.needs_prediction();
        let r = ui.add_enabled(ok, egui::Button::new("Check my solution on every seed"));
        if r.clicked() {
            code.request_submit();
        }
        if m.needs_prediction() {
            ui.label("Lock in your prediction first.");
        }
        if busy {
            ui.spinner();
            ui.label("Building and running...");
        }
    });
}

fn brief_tab(ui: &mut egui::Ui, m: &mut MissionScreen, code: &mut CodeScreen, busy: bool) {
    // Warm-up: a few questions on ideas from earlier missions that are due again.
    if !m.warmups.is_empty() {
        ui.strong("Warm-up");
        let day = m.day;
        let mut reviews: Vec<(String, bool)> = Vec::new();
        for w in &mut m.warmups {
            ui.label(&w.question.text);
            match w.correct {
                None => {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut w.input)
                                .desired_width(90.0)
                                .hint_text(w.question.unit.as_str()),
                        );
                        if ui.button("Answer").clicked() {
                            let ok = w
                                .input
                                .trim()
                                .replace(',', ".")
                                .parse::<f64>()
                                .is_ok_and(|v| w.question.is_correct(v));
                            w.correct = Some(ok);
                            reviews.push((w.question.concept.clone(), ok));
                        }
                    });
                }
                Some(ok) => {
                    ui.colored_label(
                        if ok {
                            egui::Color32::from_rgb(0x60, 0xC0, 0x70)
                        } else {
                            egui::Color32::from_rgb(0xE0, 0x80, 0x60)
                        },
                        if ok { "Right." } else { "Not quite." },
                    );
                    ui.label(format!(
                        "Answer: {:.3} {}. {}",
                        w.question.answer, w.question.unit, w.question.working
                    ));
                }
            }
            ui.add_space(4.0);
        }
        for (concept, ok) in reviews {
            m.progress.review_concept(&concept, ok, day);
            m.save();
        }
        ui.separator();
    }

    for b in results::blocks(&m.def.brief_md) {
        match b {
            Block::Heading(t) => {
                ui.heading(t);
            }
            Block::Para(t) => {
                ui.label(t);
                ui.add_space(4.0);
            }
            Block::Bullet(t) => {
                ui.label(format!("\u{2022} {t}"));
            }
        }
    }
    ui.separator();
    ui.strong("You need");
    for c in &m.def.criteria {
        ui.label(format!("\u{2022} {}", results::describe(c)));
    }
    for (i, s) in m.def.stars.iter().enumerate() {
        ui.label(format!("{} {}", stars_text(i as u8 + 2), s.text));
    }
    ui.separator();

    if let Some(p) = m.def.predict.clone() {
        ui.strong("Predict first");
        ui.label(&p.question);
        match m.predicted() {
            Some(v) => {
                ui.label(format!("Your prediction: {v} {}", p.unit));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut m.predict_input)
                            .desired_width(90.0)
                            .hint_text(p.unit.as_str()),
                    );
                    if ui.button("Lock in").clicked() {
                        match m.predict_input.trim().replace(',', ".").parse::<f64>() {
                            Ok(v) if v.is_finite() => {
                                let id = m.def.id.clone();
                                m.progress.set_prediction(&id, v);
                                m.save();
                                m.notice.clear();
                            }
                            _ => m.notice = "Type a number for your prediction.".into(),
                        }
                    }
                });
            }
        }
        ui.separator();
    }
    submit_button(ui, m, code, busy);
    ui.separator();
    hints_section(ui, m);
}

fn hints_section(ui: &mut egui::Ui, m: &mut MissionScreen) {
    ui.strong("Hints");
    let id = m.def.id.clone();
    let open = m.progress.hints_open(&id) as usize;
    if !m.progress.hints_unlocked(&id) {
        ui.label("Hints unlock after your first run that does not pass.");
        return;
    }
    for (i, h) in m.def.hints.iter().enumerate().take(open) {
        ui.label(format!("Hint {}: {h}", i + 1));
    }
    if open < 3
        && ui
            .button(format!("Open hint {} of 3", open + 1))
            .on_hover_text("A hint costs the no-hints star.")
            .clicked()
    {
        m.progress.open_next_hint(&id);
        m.save();
    }
}

fn results_tab(ui: &mut egui::Ui, m: &mut MissionScreen, code: &mut CodeScreen, busy: bool) {
    let Some(run) = m.result.clone() else {
        ui.label("Check your solution to see the results.");
        return;
    };
    let g = &run.grade;
    ui.horizontal(|ui| {
        ui.heading(if g.passed { "Passed" } else { "Not yet" });
        ui.label(
            egui::RichText::new(stars_text(g.stars))
                .size(26.0)
                .color(egui::Color32::from_rgb(0xF0, 0xC0, 0x30)),
        );
    });
    ui.separator();
    for r in results::rows(g) {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                if r.ok {
                    egui::Color32::from_rgb(0x60, 0xC0, 0x70)
                } else {
                    egui::Color32::from_rgb(0xE0, 0x70, 0x60)
                },
                if r.ok { "PASS" } else { "FAIL" },
            );
            ui.strong(&r.name);
            ui.label(&r.message);
        });
    }
    if g.passed {
        for (i, s) in m.def.stars.iter().enumerate() {
            let earned = g.stars as usize > i + 1;
            ui.label(format!(
                "{} {} ({})",
                stars_text(i as u8 + 2),
                s.text,
                if earned { "earned" } else { "not yet" }
            ));
        }
    }
    ui.separator();

    // Prediction, with the real answer.
    if let (Some(p), Some(pred)) = (m.def.predict.clone(), m.predicted())
        && let Some(actual) = run
            .runs
            .first()
            .and_then(|r| results::answer_of(&r.trace, &p.measured))
    {
        ui.strong("Your prediction");
        ui.label(results::prediction_verdict(pred, actual, &p.unit));
        ui.separator();
    }

    // Metrics.
    let traces: Vec<_> = run.runs.iter().map(|r| r.trace.clone()).collect();
    egui::Grid::new("metrics").striped(true).show(ui, |ui| {
        for h in ["Seed", "Result", "Time (s)", "Energy (Wh)", "Collisions"] {
            ui.strong(h);
        }
        ui.end_row();
        for s in results::metrics(g, &traces) {
            ui.label(s.seed.to_string());
            ui.label(if s.passed { "pass" } else { "fail" });
            ui.label(format!("{:.1}", s.time_s));
            ui.label(format!("{:.3}", s.energy_wh));
            ui.label(s.collisions.to_string());
            ui.end_row();
        }
    });
    ui.separator();

    // Replay of the recorded path.
    ui.strong("Replay");
    ui.horizontal_wrapped(|ui| {
        for (i, r) in run.runs.iter().enumerate() {
            let label = format!("seed {}", r.trace.seed);
            if ui.selectable_label(m.replay_seed == i, label).clicked() {
                m.replay_seed = i;
                m.replay_t = r.trace.last().map_or(0.0, |s| s.t);
            }
        }
    });
    if let Some(r) = run.runs.get(m.replay_seed) {
        if ui
            .button(format!("Watch seed {} in 3D", r.trace.seed))
            .on_hover_text("Runs your program again in the mission's world on this seed")
            .clicked()
        {
            m.watch = Some(r.trace.seed);
        }
        let end = r.trace.last().map_or(0.0, |s| s.t);
        ui.add(
            egui::Slider::new(&mut m.replay_t, 0.0..=end.max(0.01))
                .text("s")
                .fixed_decimals(1),
        );
        let upto: Vec<[f64; 2]> = r
            .trace
            .samples
            .iter()
            .take_while(|s| s.t <= m.replay_t)
            .map(|s| [s.pos[0], s.pos[1]])
            .collect();
        let all: Vec<[f64; 2]> = r
            .trace
            .samples
            .iter()
            .map(|s| [s.pos[0], s.pos[1]])
            .collect();
        let here = upto.last().copied();
        Plot::new("replay_plot")
            .height(220.0)
            .data_aspect(1.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .show(ui, |p| {
                p.line(Line::new("whole run", PlotPoints::from(all)));
                p.line(Line::new("so far", PlotPoints::from(upto)).width(3.0));
                if let Some(h) = here {
                    p.points(Points::new("robot", PlotPoints::from(vec![h])).radius(5.0));
                }
            });
        if let Some(err) = &r.trace.program_error {
            ui.colored_label(egui::Color32::from_rgb(0xE0, 0x70, 0x60), err);
        }
    }
    ui.separator();

    // One hint.
    if !g.passed {
        let id = m.def.id.clone();
        let open = m.progress.hints_open(&id);
        if open > 0 {
            let i = (open as usize - 1).min(2);
            ui.strong(format!("Hint {}", i + 1));
            ui.label(&m.def.hints[i]);
        } else if m.progress.hints_unlocked(&id) {
            ui.label("Stuck? Open a hint in the Brief tab.");
        }
        ui.separator();
    }
    submit_button(ui, m, code, busy);
}

/// The Results screen: the verdict of the last check, the metrics and the replay.
pub fn results_panel(
    mut contexts: EguiContexts,
    m: Option<ResMut<MissionScreen>>,
    code: Option<ResMut<CodeScreen>>,
    mut inset: ResMut<LeftInset>,
    top: Res<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut root = crate::nav::free_ui(&ctx, top.0);
    let panel = egui::Panel::left("results_panel")
        .default_size(560.0)
        .min_size(360.0)
        .show(&mut root, |ui| {
            let (Some(mut m), Some(mut code)) = (m, code) else {
                ui.heading("Results");
                ui.label("Open a mission and check your solution to see results here.");
                return;
            };
            let busy = m.grading.is_some() || code.submit_pending();
            ui.heading(format!("Mission {}: {}", m.def.id, m.def.title));
            if !m.notice.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0xA0, 0x40), &m.notice);
            }
            egui::ScrollArea::vertical()
                .id_salt("results_scroll")
                .show(ui, |ui| results_tab(ui, &mut m, &mut code, busy));
        });
    inset.0 = panel.response.rect.width();
    Ok(())
}
