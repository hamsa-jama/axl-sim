//! The egui side panel of the debug part browser (`axl --parts all`). It edits
//! [`BrowserState`]; the 3D view in `axl-render::browser` rebuilds when that changes.

use axl_render::bevy::prelude::*;
use axl_render::browser::{BrowserState, PartStats, matches_query};
use axl_render::materials_data::PART_COLORS;
use axl_render::parts::PartLibrary;
use bevy_egui::{EguiContexts, egui};

/// Width of the left side panel in egui points, so overlays can stay clear of it.
#[derive(Resource, Default, Clone, Copy)]
pub struct LeftInset(pub f32);

/// Stats of the selected part, computed once per selection.
#[derive(Default)]
pub struct StatsCache(Option<(String, PartStats)>);

/// Side panel: filter, part list, colour, overlay toggles and the selected part's numbers.
pub fn browser_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<BrowserState>,
    mut lib: ResMut<PartLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: Local<StatsCache>,
    mut inset: ResMut<LeftInset>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // Edit a copy so the resource is only marked changed when something really changed.
    let mut s = state.clone();
    let mut root = egui::Ui::new(
        ctx.clone(),
        "browser_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let rows: Vec<(String, String, String)> = lib
        .catalog()
        .iter()
        .filter(|d| matches_query(d, &s.query))
        .map(|d| (d.id.0.clone(), d.name.clone(), format!("{:?}", d.category)))
        .collect();
    let panel = egui::Panel::left("part_browser")
        .default_size(280.0)
        .show(&mut root, |ui| {
            ui.heading("Part browser");
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.text_edit_singleline(&mut s.query);
            });
            ui.horizontal_wrapped(|ui| {
                for c in [
                    "all", "brick", "plate", "tile", "beam", "axle", "pin", "gear", "wheel",
                    "caster", "hub", "motor", "sensor",
                ] {
                    if ui.small_button(c).clicked() {
                        s.query = c.to_string();
                        s.selected = None;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Colour");
                let shown = s.color.clone().unwrap_or_else(|| "default".into());
                egui::ComboBox::from_id_salt("browser_color")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.color, None, "default");
                        for (id, _) in PART_COLORS {
                            ui.selectable_value(&mut s.color, Some(id.to_string()), id);
                        }
                    });
            });
            ui.checkbox(&mut s.show_connectors, "Connectors");
            ui.checkbox(&mut s.show_collision, "Collision shapes");
            if s.selected.is_some() && ui.button("Back to contact sheet").clicked() {
                s.selected = None;
            }
            ui.separator();
            if let Some(id) = s.selected.clone() {
                let fresh = cache.0.as_ref().is_none_or(|(c, _)| *c != id);
                if fresh {
                    cache.0 = lib.stats(&id, &mut meshes).map(|st| (id.clone(), st));
                }
                if let Some((_, st)) = &cache.0 {
                    ui.strong(&id);
                    ui.label(format!("mass {:.3} g", st.mass_g));
                    match st.volume_mm3 {
                        Some(v) => ui.label(format!("volume {v:.1} mm3 (computed)")),
                        None => ui.label("volume: n/a (measured or placeholder mass)"),
                    };
                    ui.label(format!(
                        "size {:.1} x {:.1} x {:.1} mm",
                        st.size_mm[0], st.size_mm[1], st.size_mm[2]
                    ));
                    ui.label(format!("{} triangles", st.triangles));
                    ui.label(format!(
                        "{} connectors, {} collision shapes",
                        st.connectors, st.collisions
                    ));
                }
                ui.separator();
            }
            ui.label(format!("{} parts (click to isolate)", rows.len()));
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (id, name, cat) in &rows {
                    let on = s.selected.as_deref() == Some(id.as_str());
                    if ui
                        .selectable_label(on, format!("{name}  [{cat}]"))
                        .clicked()
                    {
                        s.selected = Some(id.clone());
                    }
                }
            });
        });
    inset.0 = panel.response.rect.right() - ctx.viewport_rect().left();
    if *state != s {
        *state = s;
    }
    Ok(())
}
