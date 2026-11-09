//! The desktop window (Bevy + egui). Needs a display and a GPU.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::cli::Args;

/// Frames left before the app exits by itself (smoke tests only).
#[derive(Resource)]
struct FramesLeft(Option<u32>);

/// Open the AXL window and run until it is closed.
pub fn run(args: &Args) -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "AXL".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .insert_resource(ClearColor(Color::srgb_u8(0x1e, 0x20, 0x24)))
        .insert_resource(FramesLeft(args.exit_after_frames))
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, exit_when_done)
        .add_systems(EguiPrimaryContextPass, status_bar)
        .run()
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera3d::default());
}

/// A thin bar so the empty window shows that the UI layer is alive.
fn status_bar(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::bottom("status").show(&mut viewport_ui, |ui| {
        ui.label(format!("AXL {}", env!("CARGO_PKG_VERSION")));
    });
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
