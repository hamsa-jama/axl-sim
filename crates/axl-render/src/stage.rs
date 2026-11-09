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
            .insert_resource(GlobalAmbientLight::NONE)
            .init_resource::<SnapshotBuffer>()
            .init_resource::<crate::sync::PlaybackSpeed>()
            .add_message::<BenchmarkFinished>()
            .add_plugins((crate::grid::GridPlugin, crate::camera_rig::CameraRigPlugin))
            .add_systems(
                Update,
                (crate::sync::sync_bodies, crate::scene::apply_micro_surface),
            );

        match &opts.content {
            StageContent::Lookdev => {
                app.insert_resource(SceneDef::load_lookdev());
            }
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
            app.insert_resource(PerfLog {
                seconds,
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
    lighting: Res<LightingConfig>,
    grid: Res<GridSettings>,
    quality: Res<ActiveQuality>,
    scene: Option<Res<SceneDef>>,
    stress: Option<Res<StressBoxes>>,
    start_pose: Res<StartPose>,
) {
    let root = spawn_root(&mut commands);
    spawn_floor(
        &mut commands,
        root,
        &mut meshes,
        &mut grid_mats,
        &lighting,
        &grid,
    );

    let mut parts = PartMaterials::new(&mut images);
    let mut home = CameraHome::default();
    let mut focus = None;
    {
        let mut builder = ShapeBuilder {
            meshes: &mut meshes,
            materials: &mut std_mats,
            parts: &mut parts,
            micro_on: quality.0.settings().micro_surface,
        };
        if let Some(scene) = scene {
            home = scene.home();
            focus = spawn_scene(&mut commands, root, &scene, &mut builder);
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
    spawn_view_camera(&mut commands, &mut images, &lighting, &home, quality.0);
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
    mut commands: Commands,
    mut cap: ResMut<LookdevCapture>,
    mut exit: MessageWriter<AppExit>,
) {
    cap.frame += 1;
    if cap.requested_at.is_none() && cap.frame >= settle_frames() {
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

fn run_benchmark(
    time: Res<Time<Real>>,
    mut bench: ResMut<Benchmark>,
    mut quality: ResMut<ActiveQuality>,
    mut done: Local<bool>,
    mut finished: MessageWriter<BenchmarkFinished>,
) {
    if *done {
        return;
    }
    if let Some((preset, avg_ms)) = bench.0.push_frame(time.delta_secs()) {
        *done = true;
        quality.0 = preset;
        finished.write(BenchmarkFinished { preset, avg_ms });
        tracing::info!(
            "benchmark: {avg_ms:.1} ms per frame, chose {}",
            preset.label()
        );
    }
}

fn perf_log(
    time: Res<Time<Real>>,
    mut log: ResMut<PerfLog>,
    quality: Res<ActiveQuality>,
    mut exit: MessageWriter<AppExit>,
) {
    log.frames += 1;
    // Skip the first second: shader compilation and environment filtering.
    log.elapsed += time.delta_secs();
    if log.elapsed > 1.0 {
        log.times_ms.push(time.delta_secs() * 1000.0);
    }
    if log.elapsed >= log.seconds + 1.0 {
        let mut t = log.times_ms.clone();
        t.sort_by(|a, b| a.total_cmp(b));
        let n = t.len().max(1);
        let avg = t.iter().sum::<f32>() / n as f32;
        let p95 = t.get((n as f32 * 0.95) as usize).copied().unwrap_or(0.0);
        let p99 = t.get((n as f32 * 0.99) as usize).copied().unwrap_or(0.0);
        println!(
            "perf: preset={} frames={} avg_ms={avg:.2} fps={:.1} p95_ms={p95:.2} p99_ms={p99:.2}",
            quality.0.label(),
            t.len(),
            1000.0 / avg.max(1e-6),
        );
        exit.write(AppExit::Success);
    }
}
