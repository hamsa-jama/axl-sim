//! The stage: AXL root, floor, camera, key light and the content shown on it, plus the
//! look-dev screenshot runner, the first-launch benchmark and the frame-time log.

use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::camera_rig::{ActiveQuality, CameraHome, FocusTarget, spawn_view_camera};
use crate::frames::spawn_root;
use crate::grid::{GridMaterial, GridSettings, spawn_floor};
use crate::lighting::LightingConfig;
use crate::parts::PartLibrary;
use crate::quality::{BenchmarkConfig, BenchmarkRun, QualityPreset};
use crate::scene::{
    PartMaterials, SceneDef, ShapeBuilder, StressBoxes, spawn_scene, spawn_stress_boxes,
};
use crate::sync::{BodyPose, RenderSnapshot, SnapshotBuffer};

/// What to put on the stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageContent {
    /// The look-dev scene (a brick, a beam, a gear, a wheel and the Rover).
    Lookdev,
    /// `n` boxes moving through the snapshot path, for the frame-rate check.
    Stress(usize),
    /// The part browser: a contact sheet of the parts matching the filter (empty shows all).
    Parts(crate::browser::BrowserState),
    /// The Builder screen: an empty robot to build on the grid floor.
    Builder,
    /// A compiled robot driven by snapshots (insert a [`crate::sim_view::SandboxScene`]).
    Sandbox,
}

/// Everything the render plugin needs at start-up.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Quality preset to start with.
    pub quality: QualityPreset,
    /// What to show.
    pub content: StageContent,
    /// Save a screenshot of the window here once the image has settled, then exit.
    pub screenshot: Option<PathBuf>,
    /// Run the 3-second benchmark and switch to the preset it picks.
    pub benchmark: bool,
    /// Exit after this many seconds and print a frame-time summary.
    pub perf_seconds: Option<f32>,
    /// Start in this view mode (grid opacity default follows it).
    pub view_mode: crate::grid::ViewMode,
    /// Start in this camera mode.
    pub camera_mode: crate::camera_rig::CameraMode,
    /// Replace the scene's camera: focus (m, AXL), yaw, pitch (degrees) and distance (m).
    pub camera_pose: Option<CameraHome>,
    /// Effects to switch off (`--disable`, `--safe-render`).
    pub disable: crate::diagnostics::DisableSet,
    /// Which environment-light path to run (`--env-mode`, a diagnostic).
    pub env_mode: crate::diagnostics::EnvMode,
    /// Why `quality` is what it is, for the start-up log.
    pub quality_reason: String,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            quality: QualityPreset::High,
            content: StageContent::Lookdev,
            screenshot: None,
            benchmark: false,
            perf_seconds: None,
            view_mode: crate::grid::ViewMode::Builder,
            camera_mode: crate::camera_rig::CameraMode::Orbit,
            camera_pose: None,
            disable: Default::default(),
            env_mode: Default::default(),
            quality_reason: "default".into(),
        }
    }
}

/// The preset the benchmark chose, sent once when it finishes.
#[derive(Message, Clone, Copy, Debug)]
pub struct BenchmarkFinished {
    /// Chosen preset.
    pub preset: QualityPreset,
    /// Average frame time in milliseconds.
    pub avg_ms: f32,
}

/// Frames to let the image settle before a screenshot (shader compile, environment filter, TAA).
const SETTLE_FRAMES: u32 = 90;

/// Settle frames, overridable with `AXL_SETTLE_FRAMES` for quick look-dev iteration.
fn settle_frames() -> u32 {
    std::env::var("AXL_SETTLE_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SETTLE_FRAMES)
}

/// Camera pose requested on the command line, applied over the scene's own.
#[derive(Resource)]
struct StartPose(Option<CameraHome>);

#[derive(Resource)]
struct LookdevCapture {
    path: PathBuf,
    frame: u32,
    requested_at: Option<u32>,
}

#[derive(Resource)]
struct Benchmark(BenchmarkRun);

#[derive(Resource)]
struct PerfLog {
    seconds: f32,
    waited: f32,
    started: Option<std::time::Instant>,
    elapsed: f32,
    frames: u32,
    times_ms: Vec<f32>,
}

/// Registers the grid, camera rig, body sync, stage content and the optional runners.
pub struct StagePlugin(pub RenderOptions);

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        let opts = self.0.clone();
        let lighting = LightingConfig::load_or_default();
        app.insert_resource(lighting)
            .insert_resource(opts.view_mode)
            .insert_resource(opts.camera_mode)
            .insert_resource(StartPose(opts.camera_pose))
            .insert_resource(ActiveQuality(opts.quality))
            .insert_resource(opts.disable)
            .insert_resource(opts.env_mode)
            .insert_resource(crate::diagnostics::QualityReason(
                opts.quality_reason.clone(),
            ))
            .add_plugins(crate::diagnostics::DiagnosticsPlugin)
            .insert_resource(GlobalAmbientLight::NONE)
            .insert_resource(PartLibrary::load())
            .init_resource::<SnapshotBuffer>()
            .init_resource::<crate::sync::PlaybackSpeed>()
            .add_message::<BenchmarkFinished>()
            .add_plugins((
                crate::grid::GridPlugin,
                crate::camera_rig::CameraRigPlugin {
                    no_picking: opts.disable.picking,
                },
            ))
            .add_systems(
                Update,
                (crate::sync::sync_bodies, crate::scene::apply_micro_surface),
            );

        match &opts.content {
            StageContent::Lookdev => {
                app.insert_resource(SceneDef::load_lookdev());
            }
            StageContent::Parts(state) => {
                app.add_plugins(crate::browser::BrowserPlugin(state.clone()));
            }
            StageContent::Builder => {
                app.add_plugins(crate::builder_view::BuilderPlugin);
            }
            StageContent::Sandbox => {}
            StageContent::Stress(n) => {
                app.insert_resource(StressBoxes(*n));
                app.add_systems(Update, stress_source);
            }
        }
        if let Some(path) = opts.screenshot.clone() {
            app.insert_resource(LookdevCapture {
                path,
                frame: 0,
                requested_at: None,
            });
            app.add_systems(Update, lookdev_capture);
        }
        if opts.benchmark {
            let cfg = crate::lighting::load_ron::<BenchmarkConfig>(
                &crate::lighting::assets_dir().join("render/quality.ron"),
            )
            .unwrap_or_default();
            app.insert_resource(Benchmark(BenchmarkRun::new(cfg)));
            app.add_systems(Update, run_benchmark);
        }
        if let Some(seconds) = opts.perf_seconds {
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
            app.insert_resource(PerfLog {
                seconds,
                waited: 0.0,
                started: None,
                elapsed: 0.0,
                frames: 0,
                times_ms: Vec::new(),
            });
            app.add_systems(Update, perf_log);
        }
        app.add_systems(Startup, setup_stage);
    }
}

#[allow(clippy::too_many_arguments)]
fn setup_stage(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut grid_mats: ResMut<Assets<GridMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut lib: ResMut<PartLibrary>,
    lighting: Res<LightingConfig>,
    grid: Res<GridSettings>,
    quality: Res<ActiveQuality>,
    disable: Res<crate::diagnostics::DisableSet>,
    scene: Option<Res<SceneDef>>,
    stress: Option<Res<StressBoxes>>,
    sandbox: Option<Res<crate::sim_view::SandboxScene>>,
    start_pose: Res<StartPose>,
) {
    let root = spawn_root(&mut commands);
    if disable.grid {
        crate::grid::spawn_plain_floor(&mut commands, root, &mut meshes, &mut std_mats, &lighting);
    } else {
        spawn_floor(
            &mut commands,
            root,
            &mut meshes,
            &mut grid_mats,
            &lighting,
            &grid,
        );
    }

    let mut parts = PartMaterials::new(&mut images);
    let mut home = CameraHome::default();
    let mut focus = None;
    {
        let mut builder = ShapeBuilder {
            meshes: &mut meshes,
            materials: &mut std_mats,
            parts: &mut parts,
            lib: &mut lib,
            micro_on: quality.0.settings().micro_surface && !disable.micro_normal,
        };
        if let Some(scene) = scene {
            home = scene.home();
            focus = spawn_scene(&mut commands, root, &scene, &mut builder);
        }
        if let Some(sb) = sandbox {
            home = crate::sim_view::spawn_sandbox(&mut commands, root, &sb, &mut builder);
        }
        if let Some(stress) = stress {
            home = CameraHome {
                focus: Vec3::new(0.0, 0.0, 0.0),
                yaw_deg: -50.0,
                pitch_deg: 35.0,
                distance: 1.6,
            };
            spawn_stress_boxes(&mut commands, root, stress.0, &mut builder);
        }
    }
    if let Some(pose) = start_pose.0 {
        home = pose;
    }
    commands.insert_resource(parts);
    commands.insert_resource(FocusTarget(focus));
    commands.insert_resource(home);
    spawn_view_camera(
        &mut commands,
        &mut images,
        &lighting,
        &home,
        quality.0,
        &disable,
    );
}

/// Scripted motion for the stress boxes, published as snapshots the same way the sim thread will.
fn stress_source(
    stress: Res<StressBoxes>,
    time: Res<Time<Real>>,
    mut buffer: ResMut<SnapshotBuffer>,
    mut last_pub: Local<f64>,
) {
    let t = time.elapsed_secs_f64();
    if t - *last_pub < 0.004 {
        return;
    }
    *last_pub = t;
    let n = stress.0;
    let cols = (n as f64).sqrt().ceil();
    let bodies = (0..n)
        .map(|i| {
            let (cx, cy) = ((i as f64 % cols), (i as f64 / cols).floor());
            let phase = i as f64 * 0.37;
            let hop = 0.012 + 0.04 * (t * 2.0 + phase).sin().abs();
            let yaw = t * 0.8 + phase;
            BodyPose {
                pos: [(cx - cols * 0.5) * 0.05, (cy - cols * 0.5) * 0.05, hop],
                quat: [(yaw / 2.0).cos(), 0.0, 0.0, (yaw / 2.0).sin()],
            }
        })
        .collect();
    buffer.push(RenderSnapshot { time: t, bodies }, t);
}

fn lookdev_capture(
    ready: Res<crate::diagnostics::ReadyFrames>,
    mut commands: Commands,
    mut cap: ResMut<LookdevCapture>,
    mut exit: MessageWriter<AppExit>,
) {
    cap.frame += 1;
    let steady = ready.0 >= crate::diagnostics::READY_FRAMES;
    let timed_out = cap.frame >= settle_frames() + 3000;
    if timed_out && cap.requested_at.is_none() && !steady {
        tracing::error!(
            "screenshot: pipelines were still compiling after {} frames; capturing anyway, the image may be incomplete",
            cap.frame
        );
    }
    if cap.requested_at.is_none() && cap.frame >= settle_frames() && (steady || timed_out) {
        cap.requested_at = Some(cap.frame);
        let path = cap.path.clone();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    // Give the GPU read-back and the file write a few frames, then finish.
    if let Some(at) = cap.requested_at
        && cap.frame >= at + 20
    {
        if cap.path.exists() {
            exit.write(AppExit::Success);
        } else if cap.frame >= at + 600 {
            eprintln!(
                "axl: the screenshot was not written to {}",
                cap.path.display()
            );
            exit.write(AppExit::error());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_benchmark(
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut saved_mode: Local<Option<bevy::window::PresentMode>>,
    time: Res<Time<Real>>,
    ready: Res<crate::diagnostics::ReadyFrames>,
    mut waited: Local<f32>,
    mut started: Local<bool>,
    mut bench: ResMut<Benchmark>,
    mut quality: ResMut<ActiveQuality>,
    mut done: Local<bool>,
    mut finished: MessageWriter<BenchmarkFinished>,
) {
    if *done {
        return;
    }
    if !*started {
        *started = true;
        // With vsync on, every frame takes at least one refresh interval (16.7 ms at 60 Hz) and
        // the benchmark could never tell a fast GPU from a slow one. Measure with it off.
        if let Ok(mut w) = windows.single_mut() {
            *saved_mode = Some(w.present_mode);
            w.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
        tracing::info!(
            "benchmark: started (measures the look-dev scene for a few seconds, then picks a preset)"
        );
    }
    *waited += time.delta_secs();
    // Measure only once pipelines have compiled and the environment is filtered.
    if ready.0 < crate::diagnostics::READY_FRAMES && *waited <= 120.0 {
        return;
    }
    if *waited > 120.0 {
        *done = true;
        if let (Ok(mut w), Some(m)) = (windows.single_mut(), saved_mode.take()) {
            w.present_mode = m;
        }
        tracing::warn!(
            "benchmark: gave up after {:.0} s without finishing (frames are too slow or stalled); keeping {}",
            *waited,
            quality.0.label()
        );
        return;
    }
    if let Some((preset, avg_ms)) = bench.0.push_frame(time.delta_secs()) {
        *done = true;
        if let (Ok(mut w), Some(m)) = (windows.single_mut(), saved_mode.take()) {
            w.present_mode = m;
        }
        quality.0 = preset;
        finished.write(BenchmarkFinished { preset, avg_ms });
        tracing::info!(
            "benchmark thresholds (ms per frame): Ultra <= {:.1}, High <= {:.1}, Medium <= {:.1}, else Low",
            bench.0.config().ultra_max_ms,
            bench.0.config().high_max_ms,
            bench.0.config().medium_max_ms
        );
        tracing::info!(
            "benchmark: {avg_ms:.1} ms per frame ({:.1} fps), chose {}",
            1000.0 / avg_ms.max(1e-3),
            preset.label()
        );
    }
}

/// Mean and percentile of a sorted slice.
fn pct(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() as f32 * p) as usize).min(sorted.len() - 1);
    sorted[i]
}

fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len().max(1) as f32
}

/// Escape a string for JSON.
fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[allow(clippy::too_many_arguments)]
fn perf_log(
    time: Res<Time<Real>>,
    ready: Res<crate::diagnostics::ReadyFrames>,
    drawn: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    shared: Res<crate::diagnostics::PerfShared>,
    disable: Res<crate::diagnostics::DisableSet>,
    store: Res<bevy::diagnostic::DiagnosticsStore>,
    mut log: ResMut<PerfLog>,
    quality: Res<ActiveQuality>,
    mut exit: MessageWriter<AppExit>,
) {
    log.frames += 1;
    // Measure only in steady state: no pipeline compiling for READY_FRAMES frames in a row.
    // After 60 s of waiting the run measures anyway and says so.
    log.waited += time.delta_secs();
    let steady = ready.0 >= crate::diagnostics::READY_FRAMES;
    if !steady && log.waited < 60.0 {
        return;
    }
    if !steady && log.elapsed == 0.0 {
        tracing::error!(
            "perf: pipelines never settled in 60 s; measuring anyway, compile hitches are included"
        );
    }
    log.started.get_or_insert_with(std::time::Instant::now);
    log.elapsed += time.delta_secs();
    log.times_ms.push(time.delta_secs() * 1000.0);
    if log.elapsed < log.seconds {
        return;
    }
    let mut t = log.times_ms.clone();
    t.sort_by(|a, b| a.total_cmp(b));
    let n = t.len().max(1);
    let avg = mean(&t);
    let (p50, p95, p99) = (pct(&t, 0.50), pct(&t, 0.95), pct(&t, 0.99));
    let fps = 1000.0 / avg.max(1e-6);
    println!(
        "perf: preset={} frames={} avg_ms={avg:.2} fps={fps:.1} p95_ms={p95:.2} p99_ms={p99:.2}",
        quality.0.label(),
        n,
    );
    let meshes: std::collections::HashSet<_> = drawn.iter().map(|(m, _)| m.0.id()).collect();
    let mats: std::collections::HashSet<_> = drawn.iter().map(|(_, m)| m.0.id()).collect();
    println!(
        "perf: drawn={} unique_meshes={} unique_materials={} (batches are at most meshes x materials)",
        drawn.iter().count(),
        meshes.len(),
        mats.len()
    );
    let (res, present) = windows
        .single()
        .map(|w| {
            (
                (
                    w.resolution.physical_width(),
                    w.resolution.physical_height(),
                ),
                format!("{:?}", w.present_mode),
            )
        })
        .unwrap_or(((0, 0), "unknown".into()));
    let (adapter, backend, device_type, driver, timestamps, main_ms, render_ms) = shared
        .data
        .lock()
        .map(|d| {
            // Drop the first second of samples to match the frame-time window (about 60 per second).
            let skip = |v: &Vec<f32>| {
                v.iter()
                    .skip(v.len().saturating_sub(n))
                    .copied()
                    .collect::<Vec<f32>>()
            };
            (
                d.adapter.clone(),
                d.backend.clone(),
                d.device_type.clone(),
                d.driver.clone(),
                d.timestamps,
                skip(&d.main_ms),
                skip(&d.render_ms),
            )
        })
        .unwrap_or_default();
    // Per-pass times from Bevy's render diagnostics (GPU needs timestamp queries).
    let mut gpu: Vec<(String, f64)> = Vec::new();
    let mut cpu: Vec<(String, f64)> = Vec::new();
    for d in store.iter() {
        let path = d.path().as_str().to_string();
        // A pass that stopped running (the one-off environment filter) keeps its last samples;
        // leave those out of a steady-state report.
        let fresh = match (d.measurement(), log.started) {
            (Some(m), Some(s)) => m.time >= s,
            _ => true,
        };
        if !fresh {
            continue;
        }
        if let Some(v) = d.average() {
            if let Some(p) = path.strip_suffix("/elapsed_gpu") {
                gpu.push((p.to_string(), v));
            } else if let Some(p) = path.strip_suffix("/elapsed_cpu") {
                cpu.push((p.to_string(), v));
            }
        }
    }
    // Nested passes (names like `a/b`, children of a pass already counted) stay out of totals and
    // the text summary, so the same work is not counted twice. `perf.json` keeps them, flagged.
    let gpu_total: f64 = gpu_top_level(&gpu).iter().map(|(_, v)| *v).sum();
    gpu.sort_by(|a, b| b.1.total_cmp(&a.1));
    cpu.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "perf: adapter={adapter:?} backend={backend} type={device_type} present_mode={present} resolution={}x{} disabled={:?}",
        res.0,
        res.1,
        disable.names()
    );
    println!(
        "perf: cpu main_ms={:.2} render_ms={:.2} (main thread and render thread, per frame)",
        mean(&main_ms),
        mean(&render_ms)
    );
    if gpu.is_empty() {
        println!(
            "perf: gpu per-pass times unavailable ({}); only CPU times are reported",
            if timestamps {
                "no GPU timestamps were recorded"
            } else {
                "adapter has no timestamp queries; they need Vulkan or DX12"
            }
        );
    } else {
        for line in gpu_summary_lines(&gpu, avg as f64) {
            println!("{line}");
        }
    }
    let list = |v: &[(String, f64)]| {
        v.iter()
            .map(|(p, ms)| {
                format!(
                    "{{\"pass\":{},\"ms\":{ms:.4},\"nested\":{}}}",
                    json_str(p),
                    is_nested_pass(p)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let json = format!(
        "{{\"adapter\":{},\"backend\":{},\"device_type\":{},\"driver\":{},\"preset\":{},\"resolution\":[{},{}],\"present_mode\":{},\"disabled\":[{}],\"frames\":{n},\"frame_ms\":{{\"mean\":{avg:.3},\"p50\":{p50:.3},\"p95\":{p95:.3},\"p99\":{p99:.3}}},\"fps\":{fps:.2},\"cpu_ms\":{{\"main\":{:.3},\"render\":{:.3}}},\"gpu_total_ms\":{gpu_total:.3},\"gpu_timestamps\":{},\"gpu_passes\":[{}],\"cpu_passes\":[{}],\"drawn\":{},\"unique_meshes\":{},\"unique_materials\":{}}}\n",
        json_str(&adapter),
        json_str(&backend),
        json_str(&device_type),
        json_str(&driver),
        json_str(quality.0.label()),
        res.0,
        res.1,
        json_str(&present),
        disable
            .names()
            .iter()
            .map(|s| json_str(s))
            .collect::<Vec<_>>()
            .join(","),
        mean(&main_ms),
        mean(&render_ms),
        !gpu.is_empty(),
        list(&gpu),
        list(&cpu),
        drawn.iter().count(),
        meshes.len(),
        mats.len(),
    );
    match std::fs::write("perf.json", json) {
        Ok(()) => println!("perf: wrote perf.json"),
        Err(e) => eprintln!("axl: could not write perf.json: {e}"),
    }
    exit.write(AppExit::Success);
}

/// Bevy names a pass `render/<pass>` and a child pass `render/<pass>/<child>`. Only the first form
/// is a top-level pass; children are inside a parent's time and must not be added again.
pub fn is_nested_pass(path: &str) -> bool {
    let rest = path.strip_prefix("render/").unwrap_or(path);
    rest.contains('/')
}

/// The top-level passes of a list of (path, ms), most expensive first.
pub fn gpu_top_level(gpu: &[(String, f64)]) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = gpu
        .iter()
        .filter(|(p, _)| !is_nested_pass(p))
        .cloned()
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

/// The `perf: gpu ...` lines: the total next to the frame time, then each top-level pass by cost.
pub fn gpu_summary_lines(gpu: &[(String, f64)], frame_ms: f64) -> Vec<String> {
    let top = gpu_top_level(gpu);
    let total: f64 = top.iter().map(|(_, v)| *v).sum();
    let mut out = vec![format!(
        "perf: gpu_total_ms={total:.2} frame_ms={frame_ms:.2} (top-level passes only; nested passes are children and excluded)"
    )];
    for (p, v) in top.iter().take(12) {
        out.push(format!("perf: gpu {v:6.2} ms  {p}"));
    }
    out
}

#[cfg(test)]
mod perf_summary_tests {
    use super::*;

    fn sample() -> Vec<(String, f64)> {
        [
            ("render/main_opaque_pass_3d", 20.7),
            ("render/shadow_pass", 2.2),
            ("render/main_pass", 24.0),
            ("render/main_pass/main_opaque_pass_3d", 20.7),
            ("render/bloom", 1.1),
        ]
        .iter()
        .map(|(p, v)| (p.to_string(), *v))
        .collect()
    }

    #[test]
    fn only_children_are_nested() {
        assert!(!is_nested_pass("render/main_opaque_pass_3d"));
        assert!(is_nested_pass("render/main_pass/main_opaque_pass_3d"));
        assert!(!is_nested_pass("main_opaque_pass_3d"));
    }

    #[test]
    fn summary_prints_the_total_and_each_top_level_pass_by_cost() {
        let lines = gpu_summary_lines(&sample(), 25.0);
        assert!(
            lines[0].starts_with("perf: gpu_total_ms=48.00 frame_ms=25.00"),
            "{}",
            lines[0]
        );
        assert_eq!(lines[1], "perf: gpu  24.00 ms  render/main_pass");
        assert_eq!(lines[2], "perf: gpu  20.70 ms  render/main_opaque_pass_3d");
        assert!(lines.iter().all(|l| !l.contains("main_pass/main_opaque")));
        assert_eq!(lines.len(), 5);
    }
}
