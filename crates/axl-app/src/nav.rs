//! The screens of AXL and how the app moves between them.
//!
//! One [`Nav`] resource says which of the seven screens is showing. Screens ask for things (open a
//! mission, open the sandbox, go to Results) by setting [`Nav::pending`]; [`apply_nav`] does the
//! work once per frame: it builds or drops the mission resources, swaps what the 3D stage shows
//! and resets the free-space insets.

use axl_host::toolchain::Lang;
use axl_missions::progress::Progress;
use axl_missions::{MissionDef, library};
use axl_render::bevy::prelude::*;
use axl_render::builder_view::BuilderRes;
use axl_render::camera_rig::ViewInsets;
use axl_render::grid::ViewMode;
use axl_render::instruments::{LiveView, Trails};
use axl_render::stage_swap::StageKind;
use bevy_egui::{EguiContexts, egui};

use crate::browser_ui::{LeftInset, TopInset};
use crate::code_ui::CodeScreen;
use crate::mission_ui::{MissionScreen, newest_design};
use crate::sandbox::{CodeRobot, LiveParts, open_live, open_live_with, run_program};
use crate::sim_ui::Instruments;

/// The seven screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// Continue, Missions, Sandbox, Settings and overall progress.
    Home,
    /// Every mission by chapter.
    Missions,
    /// The robot builder.
    Builder,
    /// The editor, with the mission's world beside it.
    Code,
    /// The 3D view of a running robot.
    Simulate,
    /// The verdict of a check.
    Results,
    /// Options and About.
    Settings,
}

impl Screen {
    /// All screens in the order of the navigation bar.
    pub const ALL: [Screen; 7] = [
        Screen::Home,
        Screen::Missions,
        Screen::Builder,
        Screen::Code,
        Screen::Simulate,
        Screen::Results,
        Screen::Settings,
    ];

    /// Name on the bar.
    pub fn label(self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Missions => "Missions",
            Screen::Builder => "Builder",
            Screen::Code => "Code",
            Screen::Simulate => "Simulate",
            Screen::Results => "Results",
            Screen::Settings => "Settings",
        }
    }

    /// The key (with Alt) that goes here: 1 to 7.
    pub fn key(self) -> egui::Key {
        match self {
            Screen::Home => egui::Key::Num1,
            Screen::Missions => egui::Key::Num2,
            Screen::Builder => egui::Key::Num3,
            Screen::Code => egui::Key::Num4,
            Screen::Simulate => egui::Key::Num5,
            Screen::Results => egui::Key::Num6,
            Screen::Settings => egui::Key::Num7,
        }
    }

    /// Does the 3D stage show a simulation here?
    pub fn shows_sim(self) -> bool {
        matches!(self, Screen::Code | Screen::Simulate | Screen::Results)
    }

    /// Do the view controls (quality, grid, camera) belong on this screen?
    pub fn has_view_controls(self) -> bool {
        matches!(self, Screen::Builder | Screen::Code | Screen::Simulate)
    }
}

/// Something a screen asks the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Show this screen.
    Go(Screen),
    /// Open a mission (brief, editor and its world) and go to Code.
    OpenMission(String),
    /// Open the free-play sandbox and go to Simulate.
    OpenSandbox,
    /// Simulate the design in the builder in the sandbox.
    SimulateDesign,
    /// Run the program that was graded last in the mission's world on this seed.
    Watch(u64, Vec<u8>),
}

/// Where the app is.
#[derive(Resource)]
pub struct Nav {
    /// The screens are in use (false for the look-dev, parts and stress-test views).
    pub active: bool,
    /// The screen showing.
    pub screen: Screen,
    /// The mission whose detail view is open on the Missions screen.
    pub detail: Option<String>,
    /// Settings shows About.
    pub about: bool,
    /// What to do next frame.
    pub pending: Option<Action>,
    /// The last thing that went wrong or worth saying, shown on the status line.
    pub message: String,
    /// Language of the editor when a mission opens.
    pub lang: Lang,
    /// Build and run when the first mission opens (`--code-run`).
    pub run_first: bool,
    shown: Option<Screen>,
}

impl Nav {
    /// A fresh state on `screen`.
    pub fn new(active: bool, screen: Screen) -> Self {
        Self {
            active,
            screen,
            detail: None,
            about: false,
            pending: None,
            message: String::new(),
            lang: Lang::C,
            run_first: false,
            shown: None,
        }
    }

    /// Ask for something.
    pub fn go(&mut self, a: Action) {
        self.pending = Some(a);
    }
}

/// What the Home and Missions screens read: the missions, the saved progress and the accuracy
/// report.
#[derive(Resource)]
pub struct Library {
    /// Missions by chapter.
    pub chapters: Vec<(u8, Vec<MissionDef>)>,
    /// Saved progress (re-read when Home or Missions opens).
    pub progress: Progress,
    /// Where the progress file lives.
    pub progress_path: Option<std::path::PathBuf>,
    /// The accuracy report, if one was found.
    pub report: Option<String>,
}

impl Library {
    /// Load the missions, the progress and the report.
    pub fn load() -> Self {
        let progress_path = axl_build::io::data_dir().map(|d| d.join("progress.ron"));
        let mut l = Self {
            chapters: library::chapters().unwrap_or_default(),
            progress: Progress::default(),
            progress_path,
            report: find_report(),
        };
        l.refresh();
        l
    }

    /// Read the progress file again (the mission screen may have changed it).
    pub fn refresh(&mut self) {
        if let Some(p) = &self.progress_path {
            self.progress = Progress::load(p);
        }
    }

    /// All mission ids in order.
    pub fn ids(&self) -> Vec<&str> {
        self.chapters
            .iter()
            .flat_map(|(_, ms)| ms.iter().map(|m| m.id.as_str()))
            .collect()
    }

    /// A mission by id.
    pub fn find(&self, id: &str) -> Option<&MissionDef> {
        self.chapters
            .iter()
            .flat_map(|(_, ms)| ms.iter())
            .find(|m| m.id == id)
    }

    /// The mission Continue goes to: the one opened last, else the first not yet passed, else
    /// the first.
    pub fn continue_id(&self) -> Option<String> {
        use axl_missions::progress::Status;
        self.progress
            .last_mission
            .clone()
            .filter(|id| self.find(id).is_some())
            .or_else(|| {
                self.ids()
                    .into_iter()
                    .find(|id| self.progress.status(id) != Status::Passed)
                    .map(str::to_owned)
            })
            .or_else(|| self.ids().first().map(|s| (*s).to_owned()))
    }
}

/// The latest accuracy report: from `target/validation/report.md` of a source checkout, or the
/// copy shipped beside the program (`accuracy-report.md`).
pub fn find_report() -> Option<String> {
    let mut starts: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(e) = std::env::current_exe()
        && let Some(d) = e.parent()
    {
        starts.push(d.to_path_buf());
    }
    if let Ok(c) = std::env::current_dir() {
        starts.push(c);
    }
    if let Some(r) = std::env::var_os("AXL_ROOT") {
        starts.push(r.into());
    }
    for s in starts {
        for d in s.ancestors().take(6) {
            for rel in ["accuracy-report.md", "target/validation/report.md"] {
                if let Ok(t) = std::fs::read_to_string(d.join(rel)) {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// A root `Ui` for the area between the bars, for panels to attach to.
pub fn free_ui(ctx: &egui::Context, top: f32) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        "free_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(egui::Rect::from_min_max(
                ctx.viewport_rect().min + egui::vec2(0.0, top.max(28.0)),
                ctx.viewport_rect().max - egui::vec2(0.0, 26.0),
            )),
    )
}

/// The bar with the seven screens. Alt and 1 to 7 go to them too.
pub fn nav_bar(
    mut contexts: EguiContexts,
    mut nav: ResMut<Nav>,
    mission: Option<Res<MissionScreen>>,
    mut top: ResMut<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !nav.active {
        return Ok(());
    }
    let mut root = crate::gui::viewport_ui(ctx);
    let bar = egui::Panel::top("navbar").show(&mut root, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("AXL");
            ui.separator();
            for (i, s) in Screen::ALL.into_iter().enumerate() {
                let here = nav.screen == s;
                if ui
                    .selectable_label(here, format!("{} {}", i + 1, s.label()))
                    .on_hover_text(format!("Alt+{}", i + 1))
                    .clicked()
                    && !here
                {
                    nav.pending = Some(Action::Go(s));
                }
            }
            if let Some(m) = &mission {
                ui.separator();
                ui.label(format!("Mission {}: {}", m.def().id, m.def().title));
            }
        });
    });
    top.0 = bar.response.rect.bottom() - ctx.viewport_rect().top();
    let want = ctx.input(|i| {
        if i.modifiers.alt && !i.modifiers.command {
            Screen::ALL.into_iter().find(|s| i.key_pressed(s.key()))
        } else {
            None
        }
    });
    if let Some(s) = want
        && s != nav.screen
    {
        nav.pending = Some(Action::Go(s));
    }
    Ok(())
}

/// Run condition: this screen is showing.
pub fn on(screen: Screen) -> impl Fn(Option<Res<Nav>>) -> bool + Clone {
    move |nav: Option<Res<Nav>>| nav.is_some_and(|n| n.active && n.screen == screen)
}

/// Run condition: the builder's panels show (the Builder screen, or the screens are not in use).
pub fn builder_visible(nav: Option<Res<Nav>>) -> bool {
    nav.is_none_or(|n| !n.active || n.screen == Screen::Builder)
}

/// Run condition: the simulation's overlays, time bar and plots show.
pub fn instruments_visible(nav: Option<Res<Nav>>) -> bool {
    nav.is_none_or(|n| !n.active || matches!(n.screen, Screen::Code | Screen::Simulate))
}

/// Run condition: the view controls belong on the current screen (or the screens are not in use).
pub fn view_controls(nav: Option<Res<Nav>>) -> bool {
    nav.is_none_or(|n| !n.active || n.screen.has_view_controls())
}

fn mission_seed(def: &MissionDef) -> u64 {
    def.grading_seeds.first().copied().unwrap_or(1)
}

/// The mission as the stage shows it: a mission that needs the learner's own robot (m12) shows
/// the Rover until a design is saved.
fn displayed(def: &MissionDef, design: Option<&std::path::Path>) -> MissionDef {
    let mut d = def.clone();
    if design.is_none() && matches!(d.robot, axl_missions::RobotSource::Build { .. }) {
        d.robot = axl_missions::RobotSource::Starter("rover".into());
    }
    d
}

fn install_live(commands: &mut Commands, live: LiveParts) {
    commands.insert_resource(live.scene);
    commands.insert_resource(live.world);
    commands.insert_resource(live.sim);
    commands.insert_resource(live.robot);
}

/// Do what the screens asked for and keep the stage in step with the screen.
#[allow(clippy::too_many_arguments)]
pub fn apply_nav(
    mut commands: Commands,
    mut nav: ResMut<Nav>,
    mut lib: ResMut<Library>,
    mut kind: ResMut<StageKind>,
    mut mode: ResMut<ViewMode>,
    mut inset: ResMut<LeftInset>,
    view: Option<ResMut<ViewInsets>>,
    builder: Option<Res<BuilderRes>>,
    mission: Option<ResMut<MissionScreen>>,
    robot: Option<Res<CodeRobot>>,
    mut ins: ResMut<Instruments>,
    mut trails: ResMut<Trails>,
    mut liveview: ResMut<LiveView>,
    scene: Option<Res<crate::sandbox::SandboxSim>>,
) {
    if !nav.active {
        return;
    }
    // Grading finished: show the verdict.
    if let Some(mut m) = mission {
        if m.take_just_graded() {
            nav.pending = Some(Action::Go(Screen::Results));
        }
        if let Some((seed, wasm)) = m.take_watch() {
            nav.pending = Some(Action::Watch(seed, wasm));
        }
    }
    if let Some(action) = nav.pending.take() {
        let mut reset_run = false;
        match action {
            Action::Go(s) => {
                nav.screen = s;
                match s {
                    Screen::Code if robot.is_none() => {
                        let id = lib.continue_id().unwrap_or_else(|| "m01".into());
                        nav.pending = Some(Action::OpenMission(id));
                    }
                    Screen::Simulate if scene.is_none() => nav.pending = Some(Action::OpenSandbox),
                    _ => {}
                }
            }
            Action::OpenMission(id) => {
                let run = std::mem::take(&mut nav.run_first);
                match library::mission(&id)
                    .map_err(|e| e.to_string())
                    .and_then(|def| open_mission(&mut commands, &def, nav.lang, run))
                {
                    Ok(note) => {
                        nav.screen = Screen::Code;
                        nav.message = note;
                        reset_run = true;
                    }
                    Err(e) => {
                        nav.message = format!("Could not open mission {id}: {e}");
                        nav.screen = Screen::Missions;
                    }
                }
            }
            Action::OpenSandbox => match open_live(&library::sandbox(), 1, None) {
                Ok(live) => {
                    install_live(&mut commands, live);
                    commands.remove_resource::<MissionScreen>();
                    commands.remove_resource::<CodeScreen>();
                    nav.screen = Screen::Simulate;
                    nav.message = String::new();
                    reset_run = true;
                }
                Err(e) => nav.message = format!("Could not open the sandbox: {e}"),
            },
            Action::SimulateDesign => {
                if let Some(b) = builder {
                    match open_live_with(&library::sandbox(), 1, &b.b.assembly) {
                        Ok(live) => {
                            install_live(&mut commands, live);
                            commands.remove_resource::<MissionScreen>();
                            commands.remove_resource::<CodeScreen>();
                            nav.screen = Screen::Simulate;
                            nav.message = "Simulating your design in the sandbox.".into();
                            reset_run = true;
                        }
                        Err(e) => nav.message = format!("Could not simulate the design: {e}"),
                    }
                }
            }
            Action::Watch(seed, wasm) => {
                if let Some(r) = &robot {
                    let design = newest_design();
                    let def = displayed(&r.def, design.as_deref());
                    match open_live(&def, seed, design.as_deref()).and_then(|live| {
                        let sim = run_program(&live.robot, wasm)?;
                        Ok(LiveParts { sim, ..live })
                    }) {
                        Ok(live) => {
                            install_live(&mut commands, live);
                            nav.screen = Screen::Simulate;
                            nav.message = format!("Watching seed {seed}.");
                            reset_run = true;
                        }
                        Err(e) => nav.message = format!("Could not run seed {seed}: {e}"),
                    }
                }
            }
        }
        if reset_run {
            ins.new_run();
            trails.clear();
            liveview.selected = None;
            liveview.drag = None;
        }
    }

    // Follow the screen.
    if nav.shown != Some(nav.screen) {
        nav.shown = Some(nav.screen);
        inset.0 = 0.0;
        if let Some(mut v) = view {
            v.rect = None;
        }
        if matches!(nav.screen, Screen::Home | Screen::Missions) {
            lib.refresh();
        }
        *mode = if nav.screen == Screen::Builder {
            ViewMode::Builder
        } else {
            ViewMode::Simulate
        };
    }
    let want = match nav.screen {
        Screen::Builder => StageKind::Builder,
        s if s.shows_sim() && (scene.is_some() || robot.is_some()) => StageKind::Sim,
        Screen::Simulate => StageKind::Sim,
        _ => StageKind::Backdrop,
    };
    if *kind != want {
        *kind = want;
    }
}

/// Open a mission: its progress, its editor and its world with the robot standing in it.
fn open_mission(
    commands: &mut Commands,
    def: &MissionDef,
    lang: Lang,
    run: bool,
) -> Result<String, String> {
    let design = newest_design();
    let mut screen = MissionScreen::open(&def.id)?;
    screen.mark_opened();
    let code = CodeScreen::open(&def.id, lang, run)?;
    let shown = displayed(def, design.as_deref());
    let live = open_live(&shown, mission_seed(def), design.as_deref())?;
    let note = if shown.robot == def.robot {
        String::new()
    } else {
        screen.say(
            "This mission needs a robot you built. Build one, save it, and open the mission again; until then the Rover stands in.",
        );
        "No saved design yet: the Rover stands in.".into()
    };
    install_live(commands, live);
    commands.insert_resource(screen);
    commands.insert_resource(code);
    Ok(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_screens_with_distinct_keys() {
        let mut keys: Vec<_> = Screen::ALL.iter().map(|s| s.key()).collect();
        keys.dedup();
        assert_eq!(keys.len(), 7);
        assert_eq!(Screen::ALL.len(), 7);
    }

    #[test]
    fn continue_goes_to_the_last_mission_else_the_first_unpassed() {
        let mut lib = Library {
            chapters: library::chapters().unwrap(),
            progress: Progress::default(),
            progress_path: None,
            report: None,
        };
        assert_eq!(lib.continue_id().as_deref(), Some("m01"));
        lib.progress.record_run("m01", true, 3, false, None, 1.0);
        assert_eq!(lib.continue_id().as_deref(), Some("m02"));
        lib.progress.set_last("m05");
        assert_eq!(lib.continue_id().as_deref(), Some("m05"));
        lib.progress.set_last("nonsense");
        assert_eq!(lib.continue_id().as_deref(), Some("m02"));
    }

    #[test]
    fn the_stage_follows_the_screen() {
        assert!(Screen::Code.shows_sim() && Screen::Simulate.shows_sim());
        assert!(!Screen::Home.shows_sim() && !Screen::Settings.shows_sim());
        assert!(Screen::Builder.has_view_controls() && !Screen::Missions.has_view_controls());
    }
}
