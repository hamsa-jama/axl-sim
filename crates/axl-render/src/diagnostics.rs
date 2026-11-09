//! Start-up diagnostics and the `--disable` switches used to find what a particular GPU or
//! driver cannot handle.
//!
//! Bevy already logs the wgpu adapter (`AdapterInfo { name, vendor, device_type, driver,
//! backend, .. }`) and reports shader and pipeline-creation failures at ERROR level. This
//! module adds what it does not log: the texture-format capabilities AXL relies on, the
//! camera/effect configuration actually in use and the quality preset with the reason for it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache, TextureFormat};
use bevy::render::renderer::{RenderAdapter, RenderAdapterInfo, RenderInstance};
use bevy::render::settings::{Backends, WgpuFeatures};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

/// Individual effects that can be switched off from the command line.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisableSet {
    /// The custom grid shader: the floor becomes a plain `StandardMaterial`.
    pub grid: bool,
    /// The procedural environment map: a flat ambient light replaces it.
    pub env: bool,
    /// Screen-space ambient occlusion.
    pub ssao: bool,
    /// Temporal anti-aliasing (FXAA is used instead) and the motion-vector prepass.
    pub taa: bool,
    /// Screen-space contact shadows.
    pub contact_shadows: bool,
    /// All directional-light shadows.
    pub shadows: bool,
    /// Bloom.
    pub bloom: bool,
    /// Tonemapping (and the colour grade that follows it).
    pub tonemap: bool,
    /// The micro-surface normal map on plastic.
    pub micro_normal: bool,
    /// Mesh picking (the camera anchor raycasts).
    pub picking: bool,
}

/// The names `--disable` accepts, in the order they are documented.
pub const DISABLE_NAMES: [&str; 10] = [
    "grid",
    "env",
    "ssao",
    "taa",
    "contact-shadows",
    "shadows",
    "bloom",
    "tonemap",
    "micro-normal",
    "picking",
];

impl DisableSet {
    /// Everything off: what `--safe-render` selects.
    pub fn all() -> Self {
        Self {
            grid: true,
            env: true,
            ssao: true,
            taa: true,
            contact_shadows: true,
            shadows: true,
            bloom: true,
            tonemap: true,
            micro_normal: true,
            picking: true,
        }
    }

    /// Parse a comma-separated list such as `ssao,taa`. Unknown names are an error that lists
    /// the valid ones.
    pub fn parse(list: &str) -> Result<Self, String> {
        let mut set = Self::default();
        for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match name {
                "grid" => set.grid = true,
                "env" => set.env = true,
                "ssao" => set.ssao = true,
                "taa" => set.taa = true,
                "contact-shadows" => set.contact_shadows = true,
                "shadows" => set.shadows = true,
                "bloom" => set.bloom = true,
                "tonemap" => set.tonemap = true,
                "micro-normal" => set.micro_normal = true,
                "picking" => set.picking = true,
                "all" => set = Self::all(),
                other => {
                    return Err(format!(
                        "unknown effect {other:?}; use any of: {}",
                        DISABLE_NAMES.join(", ")
                    ));
                }
            }
        }
        Ok(set)
    }

    /// The names that are switched off, for the start-up log.
    pub fn names(&self) -> Vec<&'static str> {
        let flags = [
            self.grid,
            self.env,
            self.ssao,
            self.taa,
            self.contact_shadows,
            self.shadows,
            self.bloom,
            self.tonemap,
            self.micro_normal,
            self.picking,
        ];
        DISABLE_NAMES
            .iter()
            .zip(flags)
            .filter_map(|(n, on)| on.then_some(*n))
            .collect()
    }

    /// True when nothing is disabled.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Why the quality preset is what it is, for the start-up log.
#[derive(Resource, Clone, Debug, Default)]
pub struct QualityReason(pub String);

/// Data shared between the render world and the main world for the perf report.
#[derive(Default, Debug)]
pub struct PerfData {
    /// Adapter name.
    pub adapter: String,
    /// Backend (Vulkan, Dx12, Metal, ...).
    pub backend: String,
    /// Device type (DiscreteGpu, IntegratedGpu, Cpu, ...).
    pub device_type: String,
    /// Driver name and version.
    pub driver: String,
    /// Whether GPU timestamp queries are available for per-pass timings.
    pub timestamps: bool,
    /// Render-world CPU time per frame in milliseconds.
    pub render_ms: Vec<f32>,
    /// Main-world CPU time per frame in milliseconds.
    pub main_ms: Vec<f32>,
}

/// Data shared between the render world and the main world, inserted into both.
#[derive(Resource, Clone)]
pub struct PerfShared {
    /// Adapter facts and per-frame CPU times.
    pub data: Arc<Mutex<PerfData>>,
    /// Render pipelines still queued or compiling (`usize::MAX` until the render world reports).
    pub pending: Arc<AtomicUsize>,
    /// Render pipelines that failed to compile.
    pub errors: Arc<AtomicUsize>,
}

impl Default for PerfShared {
    fn default() -> Self {
        Self {
            data: Arc::default(),
            pending: Arc::new(AtomicUsize::new(usize::MAX)),
            errors: Arc::default(),
        }
    }
}

/// Consecutive frames rendered with no pipeline waiting to compile. Screenshots and perf runs
/// start counting only once this is high enough, so compile hitches and half-built images are
/// never captured.
#[derive(Resource, Default, Debug)]
pub struct ReadyFrames(pub u32);

/// Frames of steady state required before a capture or a measurement starts.
pub const READY_FRAMES: u32 = 20;

fn track_pipelines(cache: Res<PipelineCache>, shared: Res<PerfShared>) {
    let pending = cache.waiting_pipelines().count();
    let errors = cache
        .pipelines()
        .filter(|p| {
            matches!(
                p.state,
                CachedPipelineState::Err(
                    bevy::shader::ShaderCacheError::ProcessShaderError(_)
                        | bevy::shader::ShaderCacheError::CreateShaderModule(_)
                )
            )
        })
        .count();
    shared.pending.store(pending, Ordering::Relaxed);
    shared.errors.store(errors, Ordering::Relaxed);
}

fn update_ready(shared: Res<PerfShared>, mut ready: ResMut<ReadyFrames>, mut told: Local<bool>) {
    let errors = shared.errors.load(Ordering::Relaxed);
    if errors > 0 && !*told {
        *told = true;
        error!(
            "{errors} render pipeline(s) failed to compile; the ERROR lines above name the shader and the reason"
        );
    }
    if shared.pending.load(Ordering::Relaxed) == 0 {
        ready.0 = ready.0.saturating_add(1);
    } else {
        ready.0 = 0;
    }
}

type FilteredCamera = (
    With<crate::camera_rig::ViewCamera>,
    With<bevy::light::GeneratedEnvironmentMapLight>,
    With<bevy::light::EnvironmentMapLight>,
);

/// `GeneratedEnvironmentMapLight` re-filters its cubemap on every frame (Bevy 0.20.1 has no way
/// to ask for it once). The studio never changes, so once the filtered maps exist the generator
/// component is removed and the `EnvironmentMapLight` it created keeps the results.
fn freeze_environment(
    mut commands: Commands,
    ready: Res<ReadyFrames>,
    cams: Query<Entity, FilteredCamera>,
    mut steady: Local<u32>,
) {
    let Ok(cam) = cams.single() else {
        return;
    };
    if ready.0 < 8 {
        *steady = 0;
        return;
    }
    *steady += 1;
    if *steady >= 6 {
        commands
            .entity(cam)
            .remove::<bevy::light::GeneratedEnvironmentMapLight>();
        info!(
            "environment: filtered once and frozen (the generator would otherwise run every frame)"
        );
    }
}

#[derive(Resource, Default)]
struct FrameClock(Option<Instant>);

fn render_clock_start(mut clock: ResMut<FrameClock>) {
    clock.0 = Some(Instant::now());
}

fn render_clock_end(clock: Res<FrameClock>, shared: Res<PerfShared>) {
    if let (Some(t), Ok(mut d)) = (clock.0, shared.data.lock()) {
        d.render_ms.push(t.elapsed().as_secs_f32() * 1000.0);
    }
}

fn main_clock_start(mut clock: ResMut<FrameClock>) {
    clock.0 = Some(Instant::now());
}

fn main_clock_end(clock: Res<FrameClock>, shared: Res<PerfShared>) {
    if let (Some(t), Ok(mut d)) = (clock.0, shared.data.lock()) {
        d.main_ms.push(t.elapsed().as_secs_f32() * 1000.0);
    }
}

/// Log the GPU capabilities AXL depends on (runs once in the render world).
fn log_gpu_capabilities(
    adapter: Res<RenderAdapter>,
    info: Res<RenderAdapterInfo>,
    instance: Res<RenderInstance>,
    shared: Res<PerfShared>,
) {
    // Every adapter wgpu can see, so a hybrid-graphics laptop shows what was passed over.
    let others = bevy::tasks::block_on(instance.enumerate_adapters(Backends::all()));
    for a in others {
        let i = a.get_info();
        info!(
            "gpu available: {} | backend {:?} | type {:?} | driver {}",
            i.name, i.backend, i.device_type, i.driver
        );
    }
    let timestamps = adapter.features().contains(WgpuFeatures::TIMESTAMP_QUERY);
    if let Ok(mut d) = shared.data.lock() {
        d.adapter = info.name.clone();
        d.backend = format!("{:?}", info.backend);
        d.device_type = format!("{:?}", info.device_type);
        d.driver = format!("{} {}", info.driver, info.driver_info);
        d.timestamps = timestamps;
    }
    info!(
        "gpu chosen: {} (power preference: high performance unless WGPU_POWER_PREF says otherwise) | timestamp queries: {}",
        info.name,
        if timestamps {
            "yes"
        } else {
            "no (per-pass GPU times unavailable)"
        }
    );
    info!(
        "gpu: {} | backend {:?} | type {:?} | driver {} {}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );
    let limits = adapter.limits();
    info!(
        "gpu limits: max texture 2d {} | max storage textures per stage {} | max bind groups {}",
        limits.max_texture_dimension_2d,
        limits.max_storage_textures_per_shader_stage,
        limits.max_bind_groups
    );
    for (what, format) in [
        ("hdr target / environment cube", TextureFormat::Rgba16Float),
        ("micro normal map", TextureFormat::Rgba8Unorm),
        ("depth", TextureFormat::Depth32Float),
    ] {
        let f = adapter.get_texture_format_features(format);
        info!(
            "gpu format {format:?} ({what}): usages {:?} | flags {:?}",
            f.allowed_usages, f.flags
        );
    }
}

/// Log the effective camera and effect configuration once the camera exists.
pub fn log_render_setup(
    quality: Res<crate::camera_rig::ActiveQuality>,
    reason: Res<QualityReason>,
    disable: Res<DisableSet>,
    cams: Query<(Has<bevy::camera::Hdr>, Option<&Msaa>), With<crate::camera_rig::ViewCamera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok((hdr, msaa)) = cams.single() else {
        return;
    };
    *done = true;
    if let Ok(w) = windows.single() {
        info!(
            "window: {}x{} physical, scale {:.2}, present mode {:?} (vsync {})",
            w.resolution.physical_width(),
            w.resolution.physical_height(),
            w.resolution.scale_factor(),
            w.present_mode,
            match w.present_mode {
                bevy::window::PresentMode::AutoNoVsync | bevy::window::PresentMode::Immediate =>
                    "off",
                _ => "on",
            }
        );
    }
    *done = true;
    info!(
        "render: quality {} ({}) | hdr {} | msaa {:?} | disabled: {}",
        quality.0.label(),
        reason.0,
        hdr,
        msaa,
        if disable.is_empty() {
            "none".to_string()
        } else {
            disable.names().join(",")
        }
    );
}

/// Adds the GPU capability log to the render app and the setup log to the main app.
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        let shared = PerfShared::default();
        app.init_resource::<QualityReason>()
            .init_resource::<DisableSet>()
            .init_resource::<FrameClock>()
            .insert_resource(shared.clone())
            .init_resource::<ReadyFrames>()
            .add_systems(Update, (log_render_setup, update_ready, freeze_environment))
            .add_systems(First, main_clock_start)
            .add_systems(Last, main_clock_end);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(shared)
                .init_resource::<FrameClock>()
                .add_systems(RenderStartup, log_gpu_capabilities)
                .add_systems(Render, track_pipelines.after(RenderSystems::PostCleanup))
                .add_systems(
                    Render,
                    (
                        render_clock_start.before(RenderSystems::ExtractCommands),
                        render_clock_end.after(RenderSystems::PostCleanup),
                    ),
                );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_and_rejects_typos() {
        let s = DisableSet::parse("ssao, taa,bloom").unwrap();
        assert!(s.ssao && s.taa && s.bloom && !s.grid && !s.env);
        assert_eq!(s.names(), vec!["ssao", "taa", "bloom"]);
        let err = DisableSet::parse("ssao,fog").unwrap_err();
        assert!(err.contains("fog") && err.contains("contact-shadows"));
        assert!(DisableSet::parse("").unwrap().is_empty());
    }

    #[test]
    fn all_switches_everything_off() {
        let all = DisableSet::all();
        assert_eq!(all.names().len(), DISABLE_NAMES.len());
        assert_eq!(DisableSet::parse("all").unwrap(), all);
    }

    #[test]
    fn every_documented_name_parses() {
        for n in DISABLE_NAMES {
            let s = DisableSet::parse(n).unwrap();
            assert_eq!(s.names(), vec![n]);
        }
    }
}
