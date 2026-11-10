//! The desktop window (Bevy + egui). Needs a display and a GPU.

use axl_render::bevy::prelude::*;
use axl_render::bevy::render::RenderPlugin;
use axl_render::bevy::render::settings::{PowerPreference, RenderCreation, WgpuSettings};
use axl_render::bevy::window::PresentMode;
use axl_render::camera_rig::{
    ActiveQuality, CameraHome, CameraMode, CameraSettings, UiCapture, ViewCamera, gizmo_axes,
};
use axl_render::diagnostics::{DisableSet, EnvMode};
use axl_render::grid::{CursorOnFloor, GridSettings, Measure, ViewMode};
use axl_render::grid_math::GridUnits;
use axl_render::quality::QualityPreset;
use axl_render::{BenchmarkFinished, RenderOptions, StageContent, StagePlugin};
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::browser_ui::{LeftInset, TopInset, browser_panel};
use crate::cli::Args;
use crate::settings::Settings;

/// Frames left before the app exits by itself (smoke tests only).
#[derive(Resource)]
struct FramesLeft(Option<u32>);

/// Saved settings and where they live. `path` is `None` when this run must not touch disk.
#[derive(Resource)]
struct SettingsStore {
    current: Settings,
    path: Option<std::path::PathBuf>,
}

/// Open the AXL window and run until it is closed.
pub fn run(args: &Args) -> AppExit {
    install_gpu_panic_hook();
    if let Some(msg) = unsupported_backend(std::env::var("WGPU_BACKEND").ok().as_deref()) {
        eprintln!("axl: {msg}");
        return AppExit::error();
    }
    // Screenshot, stress and perf runs never read or write the user's settings.
    let scripted = args.lookdev
        || args.parts.is_some()
        || args.stress_boxes.is_some()
        || args.perf_seconds.is_some()
        || args.sandbox.is_some()
        || args.code.is_some()
        || args.screenshot.is_some();
    let path = (!scripted).then(Settings::default_path).flatten();
    let current = match &path {
        Some(p) => Settings::load(p).unwrap_or_else(|e| {
            eprintln!("axl: {e}; using default settings and leaving the file alone");
            Settings::default()
        }),
        None => Settings::default(),
    };
    let path = path.filter(|_| true);

    let forced = args.quality.as_deref().and_then(QualityPreset::parse);
    if let Some(q) = &args.quality
        && forced.is_none()
    {
        eprintln!("axl: unknown quality {q:?}; use low, medium, high or ultra");
    }
    let quality = forced.or(current.quality).unwrap_or(QualityPreset::High);
    let reason = if forced.is_some() {
        "from --quality".to_string()
    } else if current.quality.is_some() {
        "saved in settings.ron".to_string()
    } else {
        "default; the first-launch benchmark may change it".to_string()
    };
    // The first-launch benchmark runs only for a normal start with nothing saved.
    let benchmark = !scripted && forced.is_none() && current.quality.is_none();
    if benchmark {
        eprintln!("axl: benchmark: will run on this start (no saved quality, no --quality)");
    } else {
        let why = if scripted {
            "scripted run (--lookdev, --stress-boxes, --perf-seconds or --screenshot)"
        } else if forced.is_some() {
            "--quality was given"
        } else {
            "quality already saved in settings.ron"
        };
        eprintln!("axl: benchmark: skipped, {why}");
    }
    let disable = if args.safe_render {
        DisableSet::all()
    } else {
        match args.disable.as_deref().map(DisableSet::parse) {
            Some(Ok(d)) => d,
            Some(Err(e)) => {
                eprintln!("axl: --disable: {e}");
                return AppExit::error();
            }
            None => DisableSet::default(),
        }
    };

    let env_mode = match args.env_mode.as_deref().map(EnvMode::parse) {
        Some(Ok(m)) => m,
        Some(Err(e)) => {
            eprintln!("axl: --env-mode: {e}");
            return AppExit::error();
        }
        None => EnvMode::default(),
    };
    let mut disable = disable;
    if env_mode == EnvMode::Ambient {
        disable.env = true;
    }

    let sandbox = match args.sandbox.as_deref().map(|s| {
        let volts = crate::sandbox::parse_volts(args.volts.as_deref().unwrap_or(""))?;
        crate::sandbox::start(s, &volts)
    }) {
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => {
            eprintln!("axl: --sandbox: {e}");
            return AppExit::error();
        }
        None => None,
    };

    let code = match args.code.as_deref().map(|m| {
        let lang = match args.code_lang.as_deref() {
            Some("rust") => axl_host::toolchain::Lang::Rust,
            _ => axl_host::toolchain::Lang::C,
        };
        let screen = crate::code_ui::CodeScreen::open(m, lang, args.code_run)?;
        let mission = crate::mission_ui::MissionScreen::open(m)?;
        let (scene, sim, robot) = crate::sandbox::start_code()?;
        Ok::<_, String>((screen, scene, sim, robot, mission))
    }) {
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => {
            eprintln!("axl: --code: {e}");
            return AppExit::error();
        }
        None => None,
    };

    let options = RenderOptions {
        quality,
        content: match args.stress_boxes {
            Some(n) => StageContent::Stress(n),
            None if sandbox.is_some() || code.is_some() => StageContent::Sandbox,
            None if args.builder => StageContent::Builder,
            None => match &args.parts {
                Some(q) => StageContent::Parts(axl_render::browser::BrowserState {
                    query: q.clone(),
                    selected: args.part.clone(),
                    color: args.part_color.clone(),
                    show_connectors: args.connectors,
                    show_collision: args.collision,
                }),
                None => StageContent::Lookdev,
            },
        },
        screenshot: args.screenshot.clone(),
        benchmark,
        perf_seconds: args.perf_seconds,
        view_mode: match args.view.as_deref() {
            Some("simulate") => ViewMode::Simulate,
            _ if args.code.is_some() => ViewMode::Simulate,
            _ => ViewMode::Builder,
        },
        camera_mode: match args.camera.as_deref() {
            Some("follow") => CameraMode::Follow,
            Some("top-down") => CameraMode::TopDown,
            _ => CameraMode::Orbit,
        },
        camera_pose: args.camera_pose.as_deref().and_then(parse_pose),
        disable,
        env_mode,
        quality_reason: reason,
    };

    let size = args
        .window_size
        .as_deref()
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
        .filter(|(w, h)| *w >= 320 && *h >= 240);
    let big_window = args.lookdev
        || args.stress_boxes.is_some()
        || args.parts.is_some()
        || args.builder
        || args.sandbox.is_some()
        || args.code.is_some();
    let mut app = App::new();
    // Ask for the high-performance (discrete) adapter on hybrid-graphics laptops. WGPU_POWER_PREF
    // and WGPU_BACKEND in the environment still override this, so the owner can test other adapters.
    let wgpu = WgpuSettings {
        power_preference: PowerPreference::from_env().unwrap_or(PowerPreference::HighPerformance),
        disabled_features: env_mode.disabled_features(),
        ..default()
    };
    app.add_plugins(
        DefaultPlugins
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(wgpu)),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "AXL".into(),
                    resolution: size
                        .unwrap_or(if big_window {
                            (1920, 1080)
                        } else {
                            (1280, 800)
                        })
                        .into(),
                    // Frame-rate checks must not be capped by the monitor.
                    present_mode: if args.perf_seconds.is_some() {
                        PresentMode::AutoNoVsync
                    } else {
                        PresentMode::AutoVsync
                    },
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(EguiPlugin::default())
    .insert_resource(FramesLeft(args.exit_after_frames))
    .insert_resource(ExitAfterBenchmark(args.exit_after_benchmark))
    .insert_resource(GridSettings {
        units: match args.units.as_deref() {
            Some("metric") => GridUnits::Metric,
            Some("studs") => GridUnits::Studs,
            _ => current.units,
        },
        opacity_builder: current.grid_opacity_builder,
        opacity_simulate: current.grid_opacity_simulate,
        ..default()
    })
    .insert_resource(CameraSettings {
        invert_orbit: current.invert_orbit,
    })
    .insert_resource(SettingsStore { current, path })
    .init_resource::<LeftInset>()
    .init_resource::<TopInset>()
    .init_resource::<crate::builder_ui::BuilderUi>()
    .add_plugins(StagePlugin(options))
    .add_systems(
        Update,
        (
            exit_when_done,
            forward_egui_capture,
            persist_settings,
            remember_benchmark,
        ),
    )
    .add_systems(EguiPrimaryContextPass, (control_bar, status_bar).chain())
    .add_systems(
        EguiPrimaryContextPass,
        browser_panel.run_if(resource_exists::<axl_render::browser::BrowserState>),
    )
    .add_systems(
        EguiPrimaryContextPass,
        crate::builder_ui::builder_panels
            .run_if(resource_exists::<axl_render::builder_view::BuilderRes>),
    )
    .add_systems(Update, crate::builder_ui::autosave)
    // The gizmo sits in what the panels leave free, so it never covers one.
    .add_systems(
        EguiPrimaryContextPass,
        gizmo_overlay
            .after(control_bar)
            .after(status_bar)
            .after(browser_panel),
    );
    let (script, switch_on) = match args.instruments.as_deref().map(crate::sim_ui::parse_script) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            eprintln!("axl: --instruments: {e}");
            return AppExit::error();
        }
        None => Default::default(),
    };
    if let Some((scene, sim)) = sandbox {
        app.insert_resource(scene).insert_resource(sim);
        crate::sim_ui::install(&mut app, script.clone(), &switch_on, false);
        app.add_systems(
            EguiPrimaryContextPass,
            crate::sim_ui::instruments_ui
                .after(control_bar)
                .after(status_bar),
        );
    }
    if let Some((screen, scene, sim, robot, mission)) = code {
        app.init_resource::<axl_render::camera_rig::ViewInsets>();
        app.insert_resource(screen)
            .insert_resource(scene)
            .insert_resource(sim)
            .insert_resource(robot)
            .insert_resource(mission);
        crate::sim_ui::install(&mut app, script, &switch_on, true);
        app.add_systems(
            Update,
            crate::code_ui::code_tick
                .after(crate::sim_ui::instruments_tick)
                .before(axl_render::sync::sync_bodies),
        )
        .add_systems(
            EguiPrimaryContextPass,
            crate::code_ui::code_panels
                .after(control_bar)
                .after(status_bar),
        );
        app.add_systems(
            Update,
            crate::mission_ui::mission_tick.after(crate::code_ui::code_tick),
        );
        app.add_systems(
            EguiPrimaryContextPass,
            crate::mission_ui::mission_panel.after(crate::code_ui::code_panels),
        );
        app.add_systems(
            EguiPrimaryContextPass,
            crate::sim_ui::instruments_ui.after(crate::code_ui::code_panels),
        );
    }
    app.run()
}

/// Bevy panics with "Unable to find a GPU!" when no adapter matches. Say what to do about it.
fn install_gpu_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.to_string().contains("Unable to find a GPU") {
            match std::env::var("WGPU_BACKEND") {
                Ok(b) if !b.is_empty() => eprintln!(
                    "axl: no graphics adapter was found for WGPU_BACKEND={b}. This build only includes the Vulkan, DirectX 12 and Metal backends (not OpenGL). Unset WGPU_BACKEND, or set it to vulkan, dx12 or metal."
                ),
                _ => eprintln!(
                    "axl: no usable graphics adapter was found. Check that the GPU driver is installed (on Linux, Vulkan: run vulkaninfo --summary), then try again."
                ),
            }
            // The message above is the whole story: leave with status 1 instead of a backtrace.
            std::process::exit(1);
        }
        previous(info);
    }));
}

/// Parse `fx,fy,fz,yaw,pitch,distance` (mm and degrees) into a camera home.
fn parse_pose(s: &str) -> Option<CameraHome> {
    let v: Vec<f32> = s
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [fx, fy, fz, yaw, pitch, dist] = v[..] else {
        return None;
    };
    Some(CameraHome {
        focus: Vec3::new(fx, fy, fz) * 0.001,
        yaw_deg: yaw,
        pitch_deg: pitch,
        distance: dist * 0.001,
    })
}

/// Tell the 3D view when egui is using the mouse or keyboard.
fn forward_egui_capture(wants: Res<EguiWantsInput>, mut ui: ResMut<UiCapture>) {
    ui.pointer = wants.is_pointer_over_area() || wants.is_using_pointer();
    ui.keyboard = wants.wants_keyboard_input();
}

/// A root `Ui` that panels can attach to (egui 0.36 panels need a parent `Ui`).
fn viewport_ui(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

#[allow(clippy::too_many_arguments)]
fn control_bar(
    mut contexts: EguiContexts,
    mut quality: ResMut<ActiveQuality>,
    mut grid: ResMut<GridSettings>,
    mut mode: ResMut<ViewMode>,
    mut cam_mode: ResMut<CameraMode>,
    mut cam_settings: ResMut<CameraSettings>,
    measure: Res<Measure>,
    home: Res<CameraHome>,
    mut cams: Query<&mut Transform, With<ViewCamera>>,
    mut top: ResMut<TopInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = viewport_ui(ctx);
    let bar = egui::Panel::top("controls").show(&mut root, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Quality");
            let before = quality.0;
            egui::ComboBox::from_id_salt("quality")
                .selected_text(quality.0.label())
                .show_ui(ui, |ui| {
                    for p in QualityPreset::ALL {
                        ui.selectable_value(&mut quality.0, p, p.label());
                    }
                });
            let _ = before;
            ui.separator();
            ui.label("View");
            egui::ComboBox::from_id_salt("viewmode")
                .selected_text(match *mode {
                    ViewMode::Builder => "Builder",
                    ViewMode::Simulate => "Simulate",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut *mode, ViewMode::Builder, "Builder");
                    ui.selectable_value(&mut *mode, ViewMode::Simulate, "Simulate");
                });
            ui.separator();
            ui.label("Units");
            ui.selectable_value(&mut grid.units, GridUnits::Studs, "Studs");
            ui.selectable_value(&mut grid.units, GridUnits::Metric, "mm");
            ui.separator();
            ui.label("Grid opacity");
            let mut pct = match *mode {
                ViewMode::Builder => grid.opacity_builder,
                ViewMode::Simulate => grid.opacity_simulate,
            } * 100.0;
            if ui
                .add(
                    egui::Slider::new(&mut pct, 0.0..=100.0)
                        .suffix("%")
                        .integer(),
                )
                .changed()
            {
                match *mode {
                    ViewMode::Builder => grid.opacity_builder = pct / 100.0,
                    ViewMode::Simulate => grid.opacity_simulate = pct / 100.0,
                }
            }
            ui.separator();
            ui.checkbox(&mut grid.stud_dots, "Stud dots");
            ui.checkbox(&mut grid.measure_active, "Measure");
            if grid.measure_active {
                match (measure.distance_mm(), measure.distance_studs()) {
                    (Some(mm), Some(studs)) => {
                        ui.label(format!("{mm:.1} mm = {studs:.2} studs"));
                    }
                    _ => {
                        ui.label("click two points on the floor");
                    }
                }
            }
            ui.separator();
            ui.label("Camera");
            egui::ComboBox::from_id_salt("cammode")
                .selected_text(cam_mode.label())
                .show_ui(ui, |ui| {
                    for m in [CameraMode::Orbit, CameraMode::Follow, CameraMode::TopDown] {
                        ui.selectable_value(&mut *cam_mode, m, m.label());
                    }
                });
            ui.checkbox(&mut cam_settings.invert_orbit, "Invert orbit");
            if ui.button("Reset view").clicked()
                && let Ok(mut tf) = cams.single_mut()
            {
                *tf = if *cam_mode == CameraMode::TopDown {
                    axl_render::camera_rig::top_down_transform(home.focus, home.distance)
                } else {
                    axl_render::camera_rig::orbit_transform(&home)
                };
            }
        });
    });
    top.0 = bar.response.rect.bottom() - ctx.viewport_rect().top();
    Ok(())
}

fn status_bar(
    mut contexts: EguiContexts,
    cursor: Res<CursorOnFloor>,
    quality: Res<ActiveQuality>,
    cam_mode: Res<CameraMode>,
    grid: Res<GridSettings>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = viewport_ui(ctx);
    egui::Panel::bottom("status").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("AXL {}", env!("CARGO_PKG_VERSION")));
            ui.separator();
            ui.label(format!(
                "{} | {} | {}",
                quality.0.label(),
                cam_mode.label(),
                match grid.units {
                    GridUnits::Studs => "stud grid",
                    GridUnits::Metric => "metric grid",
                }
            ));
            if let Some(p) = cursor.0 {
                ui.separator();
                ui.label(format!(
                    "x {:.1} mm   y {:.1} mm   ({:.2}, {:.2} studs)",
                    p.x * 1000.0,
                    p.y * 1000.0,
                    p.x * 125.0,
                    p.y * 125.0
                ));
            }
            ui.separator();
            ui.label(
                "Right-drag orbit | Middle-drag pan | Wheel zoom | V camera | Home reset | F focus",
            );
        });
    });
    Ok(())
}

/// The XYZ gizmo in the lower-left corner of the view (X red, Y green, Z blue).
fn gizmo_overlay(
    mut contexts: EguiContexts,
    cams: Query<&GlobalTransform, With<ViewCamera>>,
    inset: Res<LeftInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Ok(cam) = cams.single() else {
        return Ok(());
    };
    // Stay clear of the left side panel (part browser, builder palette) and the status bar.
    let view = ctx.viewport_rect();
    let origin = egui::pos2(view.left() + inset.0 + 64.0, view.bottom() - 64.0 - 28.0);
    let len = 40.0;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("axl_gizmo"),
    ));
    painter.circle_filled(origin, len + 14.0, egui::Color32::from_black_alpha(60));
    let colors = [
        egui::Color32::from_rgb(0xE5, 0x39, 0x35),
        egui::Color32::from_rgb(0x43, 0xA0, 0x47),
        egui::Color32::from_rgb(0x1E, 0x88, 0xE5),
    ];
    let names = ["X", "Y", "Z"];
    let mut axes: Vec<(usize, Vec2, f32)> = gizmo_axes(cam.rotation())
        .into_iter()
        .enumerate()
        .map(|(i, (v, depth))| (i, v, depth))
        .collect();
    // Draw the farthest axis first so near axes overlap it.
    axes.sort_by(|a, b| a.2.total_cmp(&b.2));
    for (i, v, depth) in axes {
        // Screen y grows downwards.
        let tip = origin + egui::vec2(v.x, -v.y) * len;
        let alpha = if depth < 0.0 { 140 } else { 255 };
        let c = colors[i].gamma_multiply(alpha as f32 / 255.0);
        painter.line_segment([origin, tip], egui::Stroke::new(3.0, c));
        painter.circle_filled(tip, 8.0, c);
        painter.text(
            tip,
            egui::Align2::CENTER_CENTER,
            names[i],
            egui::FontId::proportional(11.0),
            egui::Color32::WHITE,
        );
    }
    Ok(())
}

fn exit_when_done(mut left: ResMut<FramesLeft>, mut exit: MessageWriter<AppExit>) {
    if let Some(n) = left.0.as_mut() {
        if *n == 0 {
            exit.write(AppExit::Success);
        } else {
            *n -= 1;
        }
    }
}

/// Store the benchmark's choice so it runs only once.
fn remember_benchmark(
    mut done: MessageReader<BenchmarkFinished>,
    mut store: ResMut<SettingsStore>,
    exit_after: Res<ExitAfterBenchmark>,
    mut exit: MessageWriter<AppExit>,
) {
    for d in done.read() {
        store.current.quality = Some(d.preset);
        save(&store);
        match &store.path {
            Some(p) => eprintln!(
                "axl: benchmark chose {} ({:.1} ms per frame); saved to {}",
                d.preset.label(),
                d.avg_ms,
                p.display()
            ),
            None => eprintln!(
                "axl: benchmark chose {} (settings not saved)",
                d.preset.label()
            ),
        }
        if exit_after.0 {
            exit.write(AppExit::Success);
        }
    }
}

/// Exit once the first-launch benchmark has reported (`--exit-after-benchmark`).
#[derive(Resource)]
struct ExitAfterBenchmark(bool);

/// Save settings whenever the user changes a persisted option.
fn persist_settings(
    quality: Res<ActiveQuality>,
    grid: Res<GridSettings>,
    cam: Res<CameraSettings>,
    mut store: ResMut<SettingsStore>,
    mut started: Local<bool>,
) {
    // The first frame sets the baseline without writing.
    if !*started {
        *started = true;
        return;
    }
    let mut next = store.current.clone();
    // A quality chosen by the benchmark or the combo box is remembered.
    if quality.is_changed() || store.current.quality.is_some() {
        next.quality = Some(quality.0);
    }
    next.units = grid.units;
    next.grid_opacity_builder = grid.opacity_builder;
    next.grid_opacity_simulate = grid.opacity_simulate;
    next.invert_orbit = cam.invert_orbit;
    if next != store.current {
        store.current = next;
        save(&store);
    }
}

fn save(store: &SettingsStore) {
    if let Some(path) = &store.path
        && let Err(e) = store.current.save(path)
    {
        eprintln!("axl: could not save settings: {e}");
    }
}

/// Backends this build includes (Bevy is built without the OpenGL backend).
const COMPILED_BACKENDS: &[&str] = &[
    "vulkan", "vk", "dx12", "d3d12", "metal", "mtl", "all", "primary",
];

/// An explanation if `WGPU_BACKEND` names only backends this build lacks, else `None`.
fn unsupported_backend(value: Option<&str>) -> Option<String> {
    let v = value?.trim().to_lowercase();
    if v.is_empty() {
        return None;
    }
    let any_ok = v
        .split([',', '|'])
        .map(str::trim)
        .any(|b| COMPILED_BACKENDS.contains(&b));
    if any_ok {
        return None;
    }
    Some(format!(
        "WGPU_BACKEND={v} is not available: this build only includes the Vulkan, DirectX 12 and Metal backends (not OpenGL). Unset WGPU_BACKEND, or set it to vulkan, dx12 or metal."
    ))
}

#[cfg(test)]
mod backend_tests {
    use super::unsupported_backend;

    #[test]
    fn gl_is_refused_and_vulkan_accepted() {
        assert!(unsupported_backend(Some("gl")).is_some());
        assert!(unsupported_backend(Some("opengl")).is_some());
        assert!(unsupported_backend(Some("vulkan")).is_none());
        assert!(unsupported_backend(Some("vulkan,gl")).is_none());
        assert!(unsupported_backend(None).is_none());
        assert!(unsupported_backend(Some("")).is_none());
    }
}
