//! The Builder screen's egui panels: top bar, part palette, inspector, validation panel, and the
//! autosave. The 3D view and the input handling are in `axl-render::builder_view`.

use axl_build::io::{self, Autosave};
use axl_build::validate::Level;
use axl_build::{Port, TurnAxis};
use axl_render::bevy::prelude::*;
use axl_render::builder_view::BuilderRes;
use axl_render::materials_data::PART_COLORS;
use bevy_egui::{EguiContexts, egui};

use crate::browser_ui::{LeftInset, TopInset};

/// Palette and bar state that is not part of the robot.
#[derive(Resource, Default)]
pub struct BuilderUi {
    search: String,
    category: String,
    name_buf: Option<(u32, String)>,
    robot_name_buf: Option<String>,
    load_choice: Option<String>,
}

const CATEGORIES: [&str; 13] = [
    "all",
    "brick",
    "plate",
    "tile",
    "holebrick",
    "beam",
    "axle",
    "pin",
    "gear",
    "wheel",
    "caster",
    "hub",
    "motor",
];

fn swatch(ui: &mut egui::Ui, id: &str, rgb: [u8; 3], selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
    let stroke = if selected {
        egui::Stroke::new(2.5, egui::Color32::from_rgb(0xFE, 0x8A, 0x18))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_gray(90))
    };
    painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
    resp.on_hover_text(id.replace('_', " "))
}

fn starters() -> Vec<(String, std::path::PathBuf)> {
    let dir = axl_render::lighting::assets_dir().join("robots");
    axl_build::starter::STARTERS
        .iter()
        .map(|(_, stem)| {
            let label = match *stem {
                "rover" => "Rover (starter)",
                _ => "Test chassis (starter)",
            };
            (label.to_string(), dir.join(format!("{stem}.axlbot")))
        })
        .collect()
}

/// All panels of the Builder screen.
#[allow(clippy::too_many_arguments)]
pub fn builder_panels(
    mut contexts: EguiContexts,
    res: Option<ResMut<BuilderRes>>,
    mut ui_state: ResMut<BuilderUi>,
    mut inset: ResMut<LeftInset>,
    top: Res<TopInset>,
    mut nav: Option<ResMut<crate::nav::Nav>>,
) -> Result {
    let Some(mut res) = res else { return Ok(()) };
    let ctx = contexts.ctx_mut()?;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "builder_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            // Leave room for the control bar above and the status bar below (other systems).
            .max_rect(egui::Rect::from_min_max(
                ctx.viewport_rect().min + egui::vec2(0.0, top.0.max(28.0)),
                ctx.viewport_rect().max - egui::vec2(0.0, 26.0),
            )),
    );
    let res = &mut *res;
    let b = &mut res.b;
    let status = &mut res.status;

    // ---- Validation panel (bottom) ----
    egui::Panel::bottom("validation")
        .default_size(96.0)
        .show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Validation");
                ui.separator();
                ui.label(status.as_str());
            });
            let issues = b.issues().to_vec();
            if issues.is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(0x6B, 0xC0, 0x7A),
                    "No problems found. The robot is ready to simulate.",
                );
            }
            egui::ScrollArea::vertical()
                .max_height(90.0)
                .show(ui, |ui| {
                    for i in &issues {
                        let (tag, col) = match i.level {
                            Level::Error => ("Error", egui::Color32::from_rgb(0xFF, 0x6B, 0x5E)),
                            Level::Warning => {
                                ("Warning", egui::Color32::from_rgb(0xFF, 0xC1, 0x3D))
                            }
                        };
                        if ui
                            .selectable_label(
                                false,
                                egui::RichText::new(format!("{tag}: {}", i.message)).color(col),
                            )
                            .clicked()
                        {
                            b.selection = i.parts.clone();
                        }
                    }
                });
        });

    // ---- Builder bar (top) ----
    egui::Panel::top("builder_bar").show(&mut root, |ui| {
        ui.horizontal_wrapped(|ui| {
            let (nu, nr) = b.history_len();
            if ui
                .add_enabled(nu > 0, egui::Button::new("Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                b.undo();
            }
            if ui
                .add_enabled(nr > 0, egui::Button::new("Redo"))
                .on_hover_text("Ctrl+Shift+Z")
                .clicked()
            {
                b.redo();
            }
            ui.separator();
            ui.label("Robot");
            let mut name = ui_state
                .robot_name_buf
                .clone()
                .unwrap_or_else(|| b.assembly.name.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
            if r.changed() {
                ui_state.robot_name_buf = Some(name.clone());
            }
            if r.lost_focus() && name != b.assembly.name && !name.trim().is_empty() {
                b.set_name(name.trim());
                ui_state.robot_name_buf = None;
            }
            if ui.button("Save").clicked() {
                *status = match io::robot_path(&b.assembly.name) {
                    Some(p) => match io::save(&b.assembly, &p) {
                        Ok(()) => format!("Saved {}", p.display()),
                        Err(e) => e.to_string(),
                    },
                    None => "No data folder found.".into(),
                };
            }
            let mut choices = starters();
            for p in io::list_robots() {
                let label = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                choices.push((label, p));
            }
            egui::ComboBox::from_id_salt("load_robot")
                .selected_text(
                    ui_state
                        .load_choice
                        .clone()
                        .unwrap_or_else(|| "Load...".into()),
                )
                .show_ui(ui, |ui| {
                    for (label, path) in &choices {
                        if ui.selectable_label(false, label).clicked() {
                            match io::load(path, &b.catalog) {
                                Ok(a) => {
                                    *status = format!("Loaded {label}");
                                    b.load(a);
                                    ui_state.robot_name_buf = None;
                                }
                                Err(e) => *status = e.to_string(),
                            }
                            ui_state.load_choice = None;
                        }
                    }
                });
            ui.separator();
            let ok = b.can_simulate() && !b.assembly.parts.is_empty();
            if ui
                .add_enabled(ok, egui::Button::new("Simulate"))
                .on_disabled_hover_text("Fix the errors in the validation panel first.")
                .clicked()
            {
                match nav.as_mut() {
                    Some(n) => n.go(crate::nav::Action::SimulateDesign),
                    None => *status = "Open the robot in the app to simulate it.".into(),
                }
            }
            ui.separator();
            ui.label(format!("{} parts", b.assembly.parts.len()));
        });
    });

    // ---- Palette (left) ----
    let palette = egui::Panel::left("palette")
        .default_size(250.0)
        .show(&mut root, |ui| {
            ui.heading("Parts");
            ui.add(egui::TextEdit::singleline(&mut ui_state.search).hint_text("Search parts"));
            ui.horizontal_wrapped(|ui| {
                for c in CATEGORIES {
                    let on = ui_state.category == c || (ui_state.category.is_empty() && c == "all");
                    if ui.selectable_label(on, c).clicked() {
                        ui_state.category = c.to_string();
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let cur = res.default_color.clone();
                for (id, rgb) in PART_COLORS {
                    if swatch(ui, id, rgb, cur.as_deref() == Some(id)).clicked() {
                        res.default_color = Some(id.to_string());
                        b.set_ghost_color(id);
                        if b.ghost.is_none() && !b.selection.is_empty() {
                            match b.recolor_selected(id) {
                                Ok(_) => {
                                    *status = format!("Recoloured to {}", id.replace('_', " "))
                                }
                                Err(e) => *status = e.to_string(),
                            }
                        }
                    }
                }
            });
            ui.separator();
            let q = ui_state.search.trim().to_lowercase();
            let cat = if ui_state.category == "all" {
                String::new()
            } else {
                ui_state.category.clone()
            };
            let ghost_part = b
                .ghost
                .as_ref()
                .and_then(|g| g.items.first())
                .map(|i| i.part.0.clone());
            let rows: Vec<(String, String, String)> = b
                .catalog
                .iter()
                .filter(|d| {
                    let c = format!("{:?}", d.category).to_lowercase();
                    (cat.is_empty() || c == cat || (cat == "motor" && c == "sensor"))
                        && (q.is_empty()
                            || d.name.to_lowercase().contains(&q)
                            || d.id.0.contains(&q)
                            || c.contains(&q))
                })
                .map(|d| (d.id.0.clone(), d.name.clone(), format!("{:?}", d.category)))
                .collect();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (id, name, c) in &rows {
                    let on = ghost_part.as_deref() == Some(id.as_str());
                    if ui.selectable_label(on, format!("{name}  [{c}]")).clicked() {
                        match b.start_place(id, res.default_color.as_deref()) {
                            Ok(()) => *status =
                                "Click on the robot to place. R, T, Y turn the part. Esc cancels."
                                    .into(),
                            Err(e) => *status = e.to_string(),
                        }
                    }
                }
            });
        });
    inset.0 = palette.response.rect.right() - ctx.viewport_rect().left();

    // ---- Inspector (right) ----
    egui::Panel::right("inspector")
        .default_size(230.0)
        .show(&mut root, |ui| {
            ui.heading("Inspector");
            let sel = b.selection.clone();
            match sel.as_slice() {
                [] => {
                    ui.label("Click a part to select it. Shift-click adds to the selection.");
                }
                [uid] => {
                    let uid = *uid;
                    if let Some(p) = b.assembly.get(uid).cloned() {
                        let def = b.catalog.get(&p.part.0).cloned();
                        ui.strong(def.as_ref().map(|d| d.name.as_str()).unwrap_or("?"));
                        ui.label(format!("Part number {uid}"));
                        ui.label(format!(
                            "Position  {:.1}, {:.1}, {:.1} mm",
                            p.pos[0] as f64 * 0.1,
                            p.pos[1] as f64 * 0.1,
                            p.pos[2] as f64 * 0.1
                        ));
                        ui.label(format!("Orientation {} of 24", p.orient.index()));
                        if let Some(d) = &def {
                            let g: f64 = d.bodies.iter().map(|x| x.mass_kg).sum::<f64>() * 1e3;
                            ui.label(format!("Mass {g:.2} g"));
                        }
                        if let Some(dev) = &p.device {
                            ui.separator();
                            ui.strong("Device");
                            let mut buf = match &ui_state.name_buf {
                                Some((u, s)) if *u == uid => s.clone(),
                                _ => dev.name.clone(),
                            };
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                let r = ui
                                    .add(egui::TextEdit::singleline(&mut buf).desired_width(110.0));
                                if r.changed() {
                                    ui_state.name_buf = Some((uid, buf.clone()));
                                }
                                if r.lost_focus() && buf != dev.name {
                                    match b.set_device_name(uid, &buf) {
                                        Ok(()) => ui_state.name_buf = None,
                                        Err(e) => *status = e.to_string(),
                                    }
                                }
                            });
                            if let Some(port) = dev.port {
                                ui.horizontal(|ui| {
                                    ui.label("Port");
                                    egui::ComboBox::from_id_salt(("port", uid))
                                        .selected_text(port.letter().to_string())
                                        .show_ui(ui, |ui| {
                                            for q in Port::ALL {
                                                if ui
                                                    .selectable_label(
                                                        q == port,
                                                        q.letter().to_string(),
                                                    )
                                                    .clicked()
                                                {
                                                    let _ = b.set_device_port(uid, q);
                                                }
                                            }
                                        });
                                });
                            } else {
                                ui.label("The hub has the ports A to F.");
                            }
                        }
                        ui.separator();
                        ui.label("Colour");
                        ui.horizontal_wrapped(|ui| {
                            let allowed: Vec<String> = def
                                .map(|d| d.colors.iter().map(|c| c.0.clone()).collect())
                                .unwrap_or_default();
                            for (id, rgb) in PART_COLORS {
                                if allowed.iter().any(|a| a == id)
                                    && swatch(ui, id, rgb, p.color.0 == id).clicked()
                                {
                                    let _ = b.recolor_selected(id);
                                }
                            }
                        });
                    }
                }
                many => {
                    ui.label(format!("{} parts selected", many.len()));
                }
            }
            if !sel.is_empty() {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Move (G)").clicked() {
                        let _ = b.start_move();
                    }
                    if ui.button("Duplicate (Ctrl+D)").clicked() {
                        let _ = b.start_duplicate();
                    }
                    if ui.button("Delete").clicked() {
                        let _ = b.delete_selected();
                    }
                });
            }
            if b.ghost.is_some() {
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Turn R").clicked() {
                        b.rotate_ghost(TurnAxis::Z);
                    }
                    if ui.button("T").clicked() {
                        b.rotate_ghost(TurnAxis::X);
                    }
                    if ui.button("Y").clicked() {
                        b.rotate_ghost(TurnAxis::Y);
                    }
                    if ui.button("Cancel").clicked() {
                        b.cancel();
                    }
                });
            }
        });
    Ok(())
}

/// Write the robot 1 s after the last edit.
pub fn autosave(res: Option<Res<BuilderRes>>, time: Res<Time<Real>>, mut auto: Local<Autosave>) {
    let Some(res) = res else { return };
    let now = time.elapsed_secs_f64();
    if auto.poll(now, res.b.rev) {
        auto.saved(res.b.rev);
        if res.b.assembly.parts.is_empty() {
            return;
        }
        if let Some(p) = io::robot_path(&res.b.assembly.name)
            && let Err(e) = io::save(&res.b.assembly, &p)
        {
            warn!("autosave failed: {e}");
        }
    }
}
