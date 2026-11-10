//! Home, Missions and Settings (with About).

use axl_missions::progress::Status;
use axl_missions::results::{self, Block};
use axl_render::bevy::prelude::*;
use axl_render::camera_rig::{ActiveQuality, CameraSettings};
use axl_render::grid::GridSettings;
use axl_render::grid_math::GridUnits;
use axl_render::quality::QualityPreset;
use bevy_egui::{EguiContexts, egui};

use crate::browser_ui::{LeftInset, TopInset};
use crate::gui::{SettingsStore, UiScale};
use crate::nav::{Action, Library, Nav, Screen, free_ui};

const GOOD: egui::Color32 = egui::Color32::from_rgb(0x60, 0xC0, 0x70);
const WARN: egui::Color32 = egui::Color32::from_rgb(0xE0, 0xA0, 0x40);
const STAR: egui::Color32 = egui::Color32::from_rgb(0xF0, 0xC0, 0x30);

/// Stars as text: filled and empty.
pub fn stars(n: u8) -> String {
    (0..3)
        .map(|i| if i < n { '\u{2605}' } else { '\u{2606}' })
        .collect()
}

/// A tick or a cross, as words as well so colour is never the only signal.
pub fn status_text(s: Status) -> (&'static str, egui::Color32) {
    match s {
        Status::New => ("New", egui::Color32::GRAY),
        Status::Tried => ("Tried", WARN),
        Status::Passed => ("Passed", GOOD),
    }
}

fn blocks(ui: &mut egui::Ui, md: &str) {
    for b in results::blocks(md) {
        match b {
            Block::Heading(t) => {
                ui.add_space(4.0);
                ui.strong(t);
            }
            Block::Para(t) => {
                ui.label(t);
            }
            Block::Bullet(t) => {
                ui.label(format!("\u{2022} {t}"));
            }
        }
    }
}

/// Home: Continue, Missions, Sandbox, Settings and the overall progress.
pub fn home_screen(
    mut contexts: EguiContexts,
    mut nav: ResMut<Nav>,
    lib: Res<Library>,
    mut inset: ResMut<LeftInset>,
    top: Res<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut root = free_ui(&ctx, top.0);
    let panel = egui::Panel::left("home")
        .default_size(520.0)
        .min_size(360.0)
        .show(&mut root, |ui| {
            ui.add_space(12.0);
            ui.heading("AXL");
            ui.label(
                "Build a robot, write its program, watch it run on a simulator you can trust.",
            );
            ui.add_space(16.0);
            let ids = lib.ids();
            let (passed, star_count) = lib.progress.totals(ids.iter().copied());
            let cont = lib.continue_id();
            let started = lib.progress.last_mission.is_some();
            let label = match (&cont, started) {
                (Some(id), true) => format!("Continue: mission {id}"),
                (Some(id), false) => format!("Start mission {id}"),
                (None, _) => "Continue".to_owned(),
            };
            let big = egui::vec2(ui.available_width().min(360.0), 44.0);
            if ui
                .add_enabled(cont.is_some(), egui::Button::new(label).min_size(big))
                .clicked()
                && let Some(id) = cont.clone()
            {
                nav.go(Action::OpenMission(id));
            }
            if ui
                .add(egui::Button::new("Missions").min_size(big))
                .clicked()
            {
                nav.go(Action::Go(Screen::Missions));
            }
            if ui
                .add(egui::Button::new("Sandbox").min_size(big))
                .on_hover_text("Drive the Rover on an open floor, with no goal")
                .clicked()
            {
                nav.go(Action::OpenSandbox);
            }
            if ui.add(egui::Button::new("Builder").min_size(big)).clicked() {
                nav.go(Action::Go(Screen::Builder));
            }
            if ui
                .add(egui::Button::new("Settings").min_size(big))
                .clicked()
            {
                nav.go(Action::Go(Screen::Settings));
            }
            ui.add_space(16.0);
            ui.strong("Your progress");
            ui.label(format!(
                "{passed} of {} missions passed, {star_count} of {} stars",
                ids.len(),
                ids.len() * 3
            ));
            let frac = if ids.is_empty() {
                0.0
            } else {
                passed as f32 / ids.len() as f32
            };
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_width(360.0)
                    .text(format!("{}%", (frac * 100.0).round())),
            );
            if !nav.message.is_empty() {
                ui.add_space(8.0);
                ui.colored_label(WARN, &nav.message);
            }
        });
    inset.0 = panel.response.rect.width();
    Ok(())
}

/// Missions by chapter, and the detail view of one.
pub fn missions_screen(
    mut contexts: EguiContexts,
    mut nav: ResMut<Nav>,
    lib: Res<Library>,
    mut inset: ResMut<LeftInset>,
    top: Res<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut root = free_ui(&ctx, top.0);
    let panel = egui::Panel::left("missions")
        .default_size(640.0)
        .min_size(380.0)
        .show(&mut root, |ui| {
            if !nav.message.is_empty() {
                ui.colored_label(WARN, &nav.message);
            }
            let detail = nav.detail.clone().and_then(|id| lib.find(&id).cloned());
            match detail {
                Some(m) => detail_view(ui, &mut nav, &lib, &m),
                None => list_view(ui, &mut nav, &lib),
            }
        });
    inset.0 = panel.response.rect.width();
    Ok(())
}

fn list_view(ui: &mut egui::Ui, nav: &mut Nav, lib: &Library) {
    ui.heading("Missions");
    egui::ScrollArea::vertical()
        .id_salt("mission_list")
        .show(ui, |ui| {
            for (ch, ms) in &lib.chapters {
                ui.add_space(8.0);
                ui.strong(axl_missions::library::chapter_title(*ch));
                for m in ms {
                    let st = lib.progress.status(&m.id);
                    let (word, col) = status_text(st);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(format!("{}  {}", m.id, m.title))
                            .on_hover_text("Open the details")
                            .clicked()
                        {
                            nav.detail = Some(m.id.clone());
                        }
                        ui.colored_label(col, word);
                        ui.colored_label(STAR, stars(lib.progress.stars(&m.id)));
                        for c in &m.concepts {
                            ui.label(egui::RichText::new(c).small().weak());
                        }
                    });
                }
            }
        });
}

fn detail_view(ui: &mut egui::Ui, nav: &mut Nav, lib: &Library, m: &axl_missions::MissionDef) {
    ui.horizontal(|ui| {
        if ui.button("\u{2190} All missions").clicked() {
            nav.detail = None;
        }
        ui.heading(format!("{}  {}", m.id, m.title));
    });
    let (word, col) = status_text(lib.progress.status(&m.id));
    ui.horizontal(|ui| {
        ui.colored_label(col, word);
        ui.colored_label(STAR, stars(lib.progress.stars(&m.id)));
        ui.label(axl_missions::library::chapter_title(m.chapter));
    });
    if !m.concepts.is_empty() {
        ui.label(format!("Ideas: {}", m.concepts.join(", ")));
    }
    ui.add_space(4.0);
    if ui
        .add(egui::Button::new("Open this mission").min_size(egui::vec2(220.0, 36.0)))
        .clicked()
    {
        nav.go(Action::OpenMission(m.id.clone()));
    }
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("mission_detail")
        .show(ui, |ui| {
            ui.strong("Brief");
            blocks(ui, &m.brief_md);
            ui.add_space(6.0);
            ui.strong("Goal");
            ui.label(results::goal_line(m));
            ui.add_space(6.0);
            ui.strong("Rules");
            for r in results::rules(m) {
                ui.label(format!("\u{2022} {r}"));
            }
            ui.add_space(6.0);
            ui.strong("Starter robot");
            ui.label(results::robot_text(m));
            ui.add_space(6.0);
            ui.strong("Exact success criteria");
            for c in &m.criteria {
                ui.label(format!("\u{2022} {}", results::describe(c)));
            }
            for (i, s) in m.stars.iter().enumerate() {
                ui.colored_label(STAR, stars(i as u8 + 2));
                ui.label(&s.text);
                for c in &s.criteria {
                    ui.label(format!("   \u{2022} {}", results::describe(c)));
                }
                if s.no_hints {
                    ui.label("   \u{2022} No hint opened for this attempt.");
                }
            }
        });
}

/// Settings and, behind a button, About.
#[allow(clippy::too_many_arguments)]
pub fn settings_screen(
    mut contexts: EguiContexts,
    mut nav: ResMut<Nav>,
    lib: Res<Library>,
    mut quality: ResMut<ActiveQuality>,
    mut grid: ResMut<GridSettings>,
    mut cam: ResMut<CameraSettings>,
    mut scale: ResMut<UiScale>,
    mut store: ResMut<SettingsStore>,
    mut inset: ResMut<LeftInset>,
    top: Res<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut root = free_ui(&ctx, top.0);
    let panel = egui::Panel::left("settings")
        .default_size(640.0)
        .min_size(380.0)
        .show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Settings");
                ui.separator();
                if ui.selectable_label(!nav.about, "Options").clicked() {
                    nav.about = false;
                }
                if ui.selectable_label(nav.about, "About").clicked() {
                    nav.about = true;
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("settings_scroll")
                .show(ui, |ui| {
                    if nav.about {
                        about(ui, &lib);
                    } else {
                        options(
                            ui,
                            &mut quality,
                            &mut grid,
                            &mut cam,
                            &mut scale,
                            &mut store,
                        );
                    }
                });
        });
    inset.0 = panel.response.rect.width();
    Ok(())
}

fn options(
    ui: &mut egui::Ui,
    quality: &mut ActiveQuality,
    grid: &mut GridSettings,
    cam: &mut CameraSettings,
    scale: &mut UiScale,
    store: &mut SettingsStore,
) {
    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Quality");
            egui::ComboBox::from_id_salt("set_quality")
                .selected_text(quality.0.label())
                .show_ui(ui, |ui| {
                    for p in QualityPreset::ALL {
                        ui.selectable_value(&mut quality.0, p, p.label());
                    }
                });
            ui.end_row();

            ui.label("Units");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut grid.units, GridUnits::Studs, "Studs");
                ui.selectable_value(&mut grid.units, GridUnits::Metric, "mm");
            });
            ui.end_row();

            ui.label("Grid opacity, builder");
            let mut b = grid.opacity_builder * 100.0;
            if ui
                .add(egui::Slider::new(&mut b, 0.0..=100.0).suffix("%").integer())
                .changed()
            {
                grid.opacity_builder = b / 100.0;
            }
            ui.end_row();

            ui.label("Grid opacity, simulate");
            let mut s = grid.opacity_simulate * 100.0;
            if ui
                .add(egui::Slider::new(&mut s, 0.0..=100.0).suffix("%").integer())
                .changed()
            {
                grid.opacity_simulate = s / 100.0;
            }
            ui.end_row();

            ui.label("UI scale");
            let mut pct = (scale.0 * 100.0).round() as i32;
            if ui
                .add(
                    egui::Slider::new(&mut pct, 100..=200)
                        .suffix("%")
                        .step_by(10.0),
                )
                .on_hover_text("Ctrl and plus, Ctrl and minus, Ctrl and 0")
                .changed()
            {
                scale.0 = pct as f32 / 100.0;
            }
            ui.end_row();

            ui.label("Orbit");
            ui.checkbox(&mut cam.invert_orbit, "Invert orbit");
            ui.end_row();
        });

    ui.add_space(12.0);
    ui.strong("Toolchains");
    ui.label("Leave a path empty to search for it as usual.");
    let mut changed = false;
    egui::Grid::new("toolchain_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("wasi-sdk folder (C)");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut store.current.wasi_sdk_path)
                        .desired_width(320.0)
                        .hint_text("search"),
                )
                .changed();
            ui.end_row();
            ui.label("cargo program (Rust)");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut store.current.cargo_path)
                        .desired_width(320.0)
                        .hint_text("cargo on the path"),
                )
                .changed();
            ui.end_row();
        });
    if changed {
        apply_toolchain_paths(&store.current);
    }

    ui.add_space(12.0);
    ui.strong("Data folder");
    let dir = axl_build::io::data_dir();
    match &dir {
        Some(d) => {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(d.display().to_string());
                if ui.button("Copy path").clicked() {
                    ui.ctx().copy_text(d.display().to_string());
                }
            });
            ui.label("Robots, programs, progress and these settings live here.");
        }
        None => {
            ui.colored_label(WARN, "This system has no data folder.");
        }
    }
}

/// Tell the toolchain finder where the learner said things are.
pub fn apply_toolchain_paths(s: &crate::settings::Settings) {
    let path = |t: &str| {
        let t = t.trim();
        (!t.is_empty()).then(|| std::path::PathBuf::from(t))
    };
    axl_host::toolchain::set_overrides(path(&s.wasi_sdk_path), path(&s.cargo_path));
}

fn about(ui: &mut egui::Ui, lib: &Library) {
    ui.strong(format!("AXL {}", env!("CARGO_PKG_VERSION")));
    ui.label("A robot simulator with the physics checked against textbook answers.");
    ui.add_space(8.0);
    ui.strong("Accuracy report");
    match &lib.report {
        Some(md) => {
            for line in md.lines() {
                let t = line.trim();
                if t.is_empty() {
                    ui.add_space(4.0);
                } else if let Some(h) = t.trim_start_matches('#').strip_prefix(' ')
                    && t.starts_with('#')
                {
                    ui.strong(h);
                } else if t.starts_with('|') {
                    ui.monospace(t);
                } else {
                    ui.label(t.replace("**", "").replace('`', ""));
                }
            }
        }
        None => {
            ui.colored_label(
                WARN,
                "No accuracy report was found. Run `cargo xtask validate` in the source folder, or use a release package, which ships one.",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stars_are_filled_then_empty() {
        assert_eq!(stars(0), "\u{2606}\u{2606}\u{2606}");
        assert_eq!(stars(2), "\u{2605}\u{2605}\u{2606}");
    }

    #[test]
    fn every_status_has_a_word_so_colour_is_never_the_only_signal() {
        for s in [Status::New, Status::Tried, Status::Passed] {
            assert!(!status_text(s).0.is_empty());
        }
    }
}

/// Volts the sandbox sliders have set, by motor name. Cleared when the robot changes.
#[derive(Resource, Default)]
pub struct DriveVolts(pub std::collections::BTreeMap<String, f32>);

/// The Simulate screen's side panel: what is running, the program's words, and in the sandbox a
/// slider per motor.
#[allow(clippy::too_many_arguments)]
pub fn simulate_screen(
    mut contexts: EguiContexts,
    mut nav: ResMut<Nav>,
    robot: Option<Res<crate::sandbox::CodeRobot>>,
    sim: Option<Res<crate::sandbox::SandboxSim>>,
    code: Option<Res<crate::code_ui::CodeScreen>>,
    mut volts: ResMut<DriveVolts>,
    view: Option<ResMut<axl_render::camera_rig::ViewInsets>>,
    top: Res<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut root = free_ui(&ctx, top.0);
    let (Some(robot), Some(sim)) = (robot, sim) else {
        return Ok(());
    };
    if robot.is_changed() {
        volts.0.clear();
    }
    let sandbox = robot.def.id == "sandbox";
    let panel = egui::Panel::right("simulate_side")
        .default_size(320.0)
        .min_size(240.0)
        .show(&mut root, |ui| {
            ui.heading(if sandbox {
                "Sandbox".to_owned()
            } else {
                format!("Mission {}", robot.def.id)
            });
            ui.label(format!("Seed {}", robot.seed))
                .on_hover_text("The same seed as the first graded run of this mission");
            if sandbox {
                ui.separator();
                ui.strong("Drive the motors");
                let mut any = false;
                for d in &robot.setup.compiled.devices.devices {
                    if d.motor.is_none() {
                        continue;
                    }
                    any = true;
                    let v = volts.0.entry(d.name.clone()).or_insert(0.0);
                    ui.label(&d.name);
                    if ui
                        .add(egui::Slider::new(v, -9.0..=9.0).suffix(" V"))
                        .changed()
                        && let Ok(sb) = sim.0.lock()
                    {
                        sb.send(axl_sim::Command::SetVolts {
                            motor: d.name.clone(),
                            volts: f64::from(*v),
                        });
                    }
                }
                if !any {
                    ui.label("This robot has no motors.");
                }
                if ui.button("Stop all motors").clicked() {
                    for (name, v) in volts.0.iter_mut() {
                        *v = 0.0;
                        if let Ok(sb) = sim.0.lock() {
                            sb.send(axl_sim::Command::SetVolts {
                                motor: name.clone(),
                                volts: 0.0,
                            });
                        }
                    }
                }
            }
            if let Some(c) = &code {
                ui.separator();
                ui.strong("Program output");
                egui::ScrollArea::vertical()
                    .id_salt("sim_log")
                    .stick_to_bottom(true)
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for l in c.log_lines() {
                            ui.monospace(l);
                        }
                    });
                if ui.button("Back to the code").clicked() {
                    nav.go(Action::Go(Screen::Code));
                }
            }
            ui.separator();
            ui.label("Space pause, Ctrl+R reset, [ and ] speed. The panels in the view show readings and plots.");
        });
    if let Some(mut v) = view {
        let r = ctx.viewport_rect();
        let p = panel.response.rect;
        v.rect = Some([
            r.left(),
            r.top() + top.0.max(28.0),
            p.left(),
            r.bottom() - 26.0,
        ]);
    }
    Ok(())
}
