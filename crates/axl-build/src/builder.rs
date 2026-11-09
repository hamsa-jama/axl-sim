//! The builder: every edit the UI can make, as a command API with undo and redo.
//!
//! The UI and the headless tests drive this one type, so they cannot disagree about what a click
//! does. Positions are always snapped here; the UI only supplies cursor rays.

use axl_parts::catalog::Catalog;
use axl_parts::types::{Category, ColorId, DeviceModel, PartDef};

use crate::geom::{ConnIndex, WPrim, derive_connections};
use crate::model::{Assembly, Connection, DeviceInstance, PlacedPart, Port, valid_device_name};
use crate::orient::TurnAxis;
use crate::place::{
    Context, Eval, Ghost, GhostItem, Hit, Ray, evaluate, find_snap, is_valid, problem_text,
    ray_aabb,
};
use crate::validate::{Issue, validate};

/// Undo steps kept.
pub const HISTORY_DEPTH: usize = 200;

/// Why a command was refused. The text is shown to the learner.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BuildError {
    /// No ghost is active.
    #[error("Pick a part first.")]
    NoGhost,
    /// The ghost has no position yet.
    #[error("Move the part over the robot first.")]
    NotPlaced,
    /// The placement is not legal.
    #[error("{0}")]
    Invalid(String),
    /// Unknown part id.
    #[error("There is no part called {0}.")]
    UnknownPart(String),
    /// The colour is not allowed for the part.
    #[error("{0} is not a colour of this part.")]
    BadColor(String),
    /// Nothing is selected.
    #[error("Select a part first.")]
    NothingSelected,
    /// The name is not `[a-z][a-z0-9_]{{0,23}}`.
    #[error("Device names start with a letter and use a-z, 0-9 and _ (up to 24 characters).")]
    BadName,
    /// The part is not a motor or sensor.
    #[error("This part has no port.")]
    NoPort,
}

/// Why the ghost exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostKind {
    /// A new part from the palette.
    New,
    /// The selection picked up to be placed again.
    Move,
    /// Copies of the selection.
    Duplicate,
}

/// The robot being edited.
pub struct Builder {
    /// The part catalog.
    pub catalog: Catalog,
    /// The assembly.
    pub assembly: Assembly,
    /// Selected part uids.
    pub selection: Vec<u32>,
    /// The ghost under the cursor, if a part is being placed.
    pub ghost: Option<Ghost>,
    /// What the ghost is.
    pub ghost_kind: GhostKind,
    /// "Underneath" aiming (`U`): the cursor ray goes through the robot to its underside, so a part
    /// can hang below a plate that rests on the floor.
    pub underneath: bool,
    /// Counts every change of the assembly (autosave and renderer watch it).
    pub rev: u64,
    undo: Vec<(String, Assembly)>,
    redo: Vec<(String, Assembly)>,
    /// Assembly before a move was picked up; restored if the move is cancelled.
    carry_origin: Option<Assembly>,
    /// Facts cached for the current assembly (the ghost is checked against these).
    ctx: Context,
    conns: Vec<Connection>,
    issues: Vec<Issue>,
}

impl Builder {
    /// A builder with an empty robot.
    pub fn new(catalog: Catalog) -> Self {
        Self::with_assembly(catalog, Assembly::default())
    }

    /// A builder editing `assembly`.
    pub fn with_assembly(catalog: Catalog, assembly: Assembly) -> Self {
        let mut b = Builder {
            catalog,
            assembly,
            selection: vec![],
            ghost: None,
            ghost_kind: GhostKind::New,
            underneath: false,
            rev: 0,
            undo: vec![],
            redo: vec![],
            carry_origin: None,
            ctx: Context::default(),
            conns: vec![],
            issues: vec![],
        };
        b.refresh();
        b
    }

    fn refresh(&mut self) {
        self.ctx = Context::build(&self.catalog, &self.assembly);
        self.conns = self.ctx.index.connections();
        self.issues = validate(&self.catalog, &self.assembly, &self.ctx.index, &self.conns);
    }

    /// Derived connections.
    pub fn connections(&self) -> &[Connection] {
        &self.conns
    }

    /// Validation issues for the current assembly.
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }

    /// Connector index of the current assembly.
    pub fn index(&self) -> &ConnIndex {
        &self.ctx.index
    }

    /// Collision shapes per part.
    pub fn prims(&self) -> &[(u32, Vec<(u8, WPrim)>)] {
        &self.ctx.prims
    }

    /// The surface under a cursor ray (a part's collision bounds or the floor).
    pub fn hit(&self, ray: &Ray) -> Option<Hit> {
        self.ctx.hit(ray)
    }

    /// The lowest point of the robot, mm.
    pub fn floor_z_mm(&self) -> f64 {
        self.ctx.floor_z
    }

    /// Can the robot be simulated (no errors)?
    pub fn can_simulate(&self) -> bool {
        !crate::validate::has_errors(&self.issues)
    }

    fn def(&self, id: &str) -> Result<&PartDef, BuildError> {
        self.catalog
            .get(id)
            .ok_or_else(|| BuildError::UnknownPart(id.into()))
    }

    fn edit(&mut self, label: &str, f: impl FnOnce(&mut Assembly)) {
        self.undo.push((label.into(), self.assembly.clone()));
        if self.undo.len() > HISTORY_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        f(&mut self.assembly);
        self.rev += 1;
        self.refresh();
    }

    /// Undo the last edit. Returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let (label, before) = self.undo.pop()?;
        self.step_history(label, before, true)
    }

    /// Redo the last undone edit.
    pub fn redo(&mut self) -> Option<String> {
        let (label, after) = self.redo.pop()?;
        self.step_history(label, after, false)
    }

    fn step_history(&mut self, label: String, target: Assembly, undoing: bool) -> Option<String> {
        let mut restored = target;
        // uids are never reused, even across undo.
        restored.next_uid = restored.next_uid.max(self.assembly.next_uid);
        let current = std::mem::replace(&mut self.assembly, restored);
        if undoing {
            self.redo.push((label.clone(), current));
        } else {
            self.undo.push((label.clone(), current));
        }
        self.selection.retain(|u| self.assembly.get(*u).is_some());
        self.ghost = None;
        self.carry_origin = None;
        self.rev += 1;
        self.refresh();
        Some(label)
    }

    /// Steps available to undo and redo.
    pub fn history_len(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    // ---- Placing ----------------------------------------------------------------------------

    /// Start placing a new part (a ghost follows the cursor). `color` defaults to the part's first.
    pub fn start_place(&mut self, part: &str, color: Option<&str>) -> Result<(), BuildError> {
        let def = self.def(part)?;
        let part_id = def.id.clone();
        let color = match color {
            Some(c) => {
                let id = ColorId(c.into());
                if !def.colors.contains(&id) {
                    return Err(BuildError::BadColor(c.into()));
                }
                id
            }
            None => def.colors[0].clone(),
        };
        self.cancel();
        self.underneath = false;
        self.ghost = Some(Ghost::single(&part_id, &color));
        self.ghost_kind = GhostKind::New;
        Ok(())
    }

    /// `G`: pick the selection up to place it again.
    pub fn start_move(&mut self) -> Result<(), BuildError> {
        self.start_group(GhostKind::Move)
    }

    /// `Ctrl+D`: carry copies of the selection.
    pub fn start_duplicate(&mut self) -> Result<(), BuildError> {
        self.start_group(GhostKind::Duplicate)
    }

    fn start_group(&mut self, kind: GhostKind) -> Result<(), BuildError> {
        if self.selection.is_empty() {
            return Err(BuildError::NothingSelected);
        }
        let mut sel: Vec<&PlacedPart> = self
            .assembly
            .parts
            .iter()
            .filter(|p| self.selection.contains(&p.uid))
            .collect();
        sel.sort_by_key(|p| p.uid);
        let Some(first) = sel.first() else {
            return Err(BuildError::NothingSelected);
        };
        let anchor = first.pos;
        let items: Vec<GhostItem> = sel
            .iter()
            .map(|p| GhostItem {
                part: p.part.clone(),
                color: p.color.clone(),
                orient: p.orient,
                rel: [0, 1, 2].map(|k| p.pos[k] - anchor[k]),
                device: if kind == GhostKind::Move {
                    p.device.clone()
                } else {
                    None
                },
                from_uid: (kind == GhostKind::Move).then_some(p.uid),
            })
            .collect();
        let moved: Vec<u32> = sel.iter().map(|p| p.uid).collect();
        self.cancel();
        if kind == GhostKind::Move {
            self.carry_origin = Some(self.assembly.clone());
            let mut base = self.assembly.clone();
            base.parts.retain(|p| !moved.contains(&p.uid));
            self.assembly = base;
            self.refresh();
        }
        self.ghost_kind = kind;
        self.ghost = Some(Ghost {
            items,
            anchor,
            placed: false,
            snap: None,
            valid: false,
            problem: None,
        });
        Ok(())
    }

    /// Drop the ghost. A cancelled move puts the parts back where they were.
    pub fn cancel(&mut self) {
        if let Some(orig) = self.carry_origin.take() {
            self.assembly = orig;
            self.refresh();
        }
        self.ghost = None;
    }

    /// `R`, `T`, `Y`: turn the ghost.
    pub fn rotate_ghost(&mut self, about: TurnAxis) {
        if let Some(g) = &mut self.ghost {
            g.turn(about);
        }
    }

    /// Cycle the ghost's colour among the colours its part allows (single-part ghosts).
    pub fn set_ghost_color(&mut self, color: &str) {
        if let Some(g) = &mut self.ghost
            && g.items.len() == 1
            && self
                .catalog
                .get(&g.items[0].part.0)
                .is_some_and(|d| d.colors.contains(&ColorId(color.into())))
        {
            g.items[0].color = ColorId(color.into());
        }
    }

    /// Point the ghost at a cursor ray: find the surface, snap, and check the placement.
    pub fn hover(&mut self, ray: &Ray) {
        let Some(mut ghost) = self.ghost.take() else {
            return;
        };
        ghost.snap = None;
        ghost.valid = false;
        ghost.problem = None;
        ghost.placed = false;
        let hit = if self.underneath {
            self.ctx.hit_far(ray)
        } else {
            self.ctx.hit(ray)
        };
        if let (Some(hit), Some((lo, hi))) = (hit, ghost.bounds_rel(&self.catalog)) {
            let free = free_anchor(&hit, lo, hi);
            ghost.placed = true;
            if self.ctx.count == 0 {
                // The first part sits on the stud grid, resting on the floor.
                ghost.anchor = [
                    (free[0] as f64 / 80.0).round() as i32 * 80,
                    (free[1] as f64 / 80.0).round() as i32 * 80,
                    free[2],
                ];
            } else if let Some((anchor, snap)) = find_snap(
                &self.catalog,
                &self.ctx,
                &ghost.items,
                &push_in(&hit, free),
                ray,
            ) {
                ghost.anchor = anchor;
                ghost.snap = Some(snap);
            } else {
                ghost.anchor = free;
            }
            let ev = evaluate(&self.catalog, &self.ctx, &ghost.items, ghost.anchor);
            ghost.valid = is_valid(&self.ctx, &ev);
            ghost.problem = problem_text(&self.ctx, &ev);
        }
        self.ghost = Some(ghost);
    }

    /// The overlaps of the current ghost placement (for highlighting).
    pub fn ghost_eval(&self) -> Option<Eval> {
        let g = self.ghost.as_ref()?;
        g.placed
            .then(|| evaluate(&self.catalog, &self.ctx, &g.items, g.anchor))
    }

    /// Click: place the ghost. Returns the uids of the new or moved parts. The ghost stays active
    /// for new parts and duplicates (so several can be placed in a row) and ends for moves.
    pub fn click_place(&mut self) -> Result<Vec<u32>, BuildError> {
        let ghost = self.ghost.clone().ok_or(BuildError::NoGhost)?;
        if !ghost.placed {
            return Err(BuildError::NotPlaced);
        }
        let ev = evaluate(&self.catalog, &self.ctx, &ghost.items, ghost.anchor);
        if !is_valid(&self.ctx, &ev) {
            return Err(BuildError::Invalid(
                problem_text(&self.ctx, &ev).unwrap_or_default(),
            ));
        }
        let kind = self.ghost_kind;
        let label = match kind {
            GhostKind::New => "Place part",
            GhostKind::Move => "Move",
            GhostKind::Duplicate => "Duplicate",
        };
        // For a move, `self.assembly` is the robot without the carried parts and the history entry
        // must hold the robot from before the pick-up.
        let before = self
            .carry_origin
            .take()
            .unwrap_or_else(|| self.assembly.clone());
        let mut result = self.assembly.clone();
        let mut devices: Vec<DeviceInstance> = result
            .parts
            .iter()
            .filter_map(|p| p.device.clone())
            .collect();
        let mut new_uids = Vec::new();
        for it in &ghost.items {
            let Some(def) = self.catalog.get(&it.part.0) else {
                continue;
            };
            let device = match (&it.device, def.device.as_ref()) {
                (Some(d), _) => Some(d.clone()),
                (None, Some(dd)) => {
                    let d = default_device(dd.kind, &devices);
                    devices.push(d.clone());
                    Some(d)
                }
                (None, None) => None,
            };
            let pp = PlacedPart {
                uid: 0,
                part: it.part.clone(),
                pos: [0, 1, 2].map(|k| ghost.anchor[k] + it.rel[k]),
                orient: it.orient,
                color: it.color.clone(),
                device,
            };
            let uid = match it.from_uid {
                Some(u) => {
                    result.parts.push(PlacedPart { uid: u, ..pp });
                    u
                }
                None => result.push(pp),
            };
            new_uids.push(uid);
        }
        result.parts.sort_by_key(|p| p.uid);
        result.next_uid = result.next_uid.max(before.next_uid);
        self.undo.push((label.into(), before));
        if self.undo.len() > HISTORY_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.assembly = result;
        if kind == GhostKind::Move {
            self.ghost = None;
        }
        self.rev += 1;
        self.refresh();
        self.selection = new_uids.clone();
        Ok(new_uids)
    }

    // ---- Selecting and editing ----------------------------------------------------------------

    /// Part under a cursor ray.
    pub fn pick(&self, ray: &Ray) -> Option<u32> {
        let mut best: Option<(f64, u32)> = None;
        for (uid, ps) in &self.ctx.prims {
            for (_, p) in ps {
                let (lo, hi) = p.aabb();
                if let Some((t, ..)) = ray_aabb(ray, lo, hi)
                    && best.is_none_or(|(bt, _)| t < bt)
                {
                    best = Some((t, *uid));
                }
            }
        }
        best.map(|(_, u)| u)
    }

    /// Click on a part: select it, or with `additive` (Shift) toggle it in the selection.
    pub fn select(&mut self, uid: Option<u32>, additive: bool) {
        match (uid, additive) {
            (None, false) => self.selection.clear(),
            (None, true) => {}
            (Some(u), false) => self.selection = vec![u],
            (Some(u), true) => {
                if let Some(i) = self.selection.iter().position(|s| *s == u) {
                    self.selection.remove(i);
                } else {
                    self.selection.push(u);
                }
            }
        }
    }

    /// `Delete`: remove the selected parts.
    pub fn delete_selected(&mut self) -> Result<usize, BuildError> {
        if self.selection.is_empty() {
            return Err(BuildError::NothingSelected);
        }
        let sel = std::mem::take(&mut self.selection);
        let n = sel.len();
        self.edit("Delete", |a| a.parts.retain(|p| !sel.contains(&p.uid)));
        Ok(n)
    }

    /// Recolour the selection. Parts that do not offer the colour keep theirs.
    pub fn recolor_selected(&mut self, color: &str) -> Result<usize, BuildError> {
        if self.selection.is_empty() {
            return Err(BuildError::NothingSelected);
        }
        let id = ColorId(color.into());
        let sel = self.selection.clone();
        let ok: Vec<u32> = sel
            .into_iter()
            .filter(|u| {
                self.assembly
                    .get(*u)
                    .and_then(|p| self.catalog.get(&p.part.0))
                    .is_some_and(|d| d.colors.contains(&id))
            })
            .filter(|u| self.assembly.get(*u).is_some_and(|p| p.color != id))
            .collect();
        if ok.is_empty() {
            return Err(BuildError::BadColor(color.into()));
        }
        let n = ok.len();
        self.edit("Recolour", |a| {
            for u in &ok {
                if let Some(p) = a.get_mut(*u) {
                    p.color = id.clone();
                }
            }
        });
        Ok(n)
    }

    /// Rename a device.
    pub fn set_device_name(&mut self, uid: u32, name: &str) -> Result<(), BuildError> {
        if !valid_device_name(name) {
            return Err(BuildError::BadName);
        }
        let has = self.assembly.get(uid).is_some_and(|p| p.device.is_some());
        if !has {
            return Err(BuildError::NoPort);
        }
        let name = name.to_string();
        self.edit("Rename device", |a| {
            if let Some(d) = a.get_mut(uid).and_then(|p| p.device.as_mut()) {
                d.name = name;
            }
        });
        Ok(())
    }

    /// Change a device's port.
    pub fn set_device_port(&mut self, uid: u32, port: Port) -> Result<(), BuildError> {
        let ok = self
            .assembly
            .get(uid)
            .and_then(|p| p.device.as_ref())
            .is_some_and(|d| d.port.is_some());
        if !ok {
            return Err(BuildError::NoPort);
        }
        self.edit("Change port", |a| {
            if let Some(d) = a.get_mut(uid).and_then(|p| p.device.as_mut()) {
                d.port = Some(port);
            }
        });
        Ok(())
    }

    /// Rename the robot.
    pub fn set_name(&mut self, name: &str) {
        let name = name.to_string();
        self.edit("Rename robot", |a| a.name = name);
    }

    /// Replace the whole assembly (load). Clears history.
    pub fn load(&mut self, assembly: Assembly) {
        self.assembly = assembly;
        self.selection.clear();
        self.ghost = None;
        self.carry_origin = None;
        self.undo.clear();
        self.redo.clear();
        self.rev += 1;
        self.refresh();
    }

    /// Does this part exist and is it a device?
    pub fn is_device(&self, part: &str) -> bool {
        self.catalog.get(part).is_some_and(|d| d.device.is_some())
    }
}

/// The resting position and the same position pushed into the surface by 4 mm steps up to 24 mm.
fn push_in(hit: &Hit, free: [i32; 3]) -> Vec<[i32; 3]> {
    let sign = if hit.positive { -1 } else { 1 };
    (0..=6)
        .map(|n| {
            let mut f = free;
            f[hit.axis] += sign * n * 40;
            f
        })
        .collect()
}

fn free_anchor(hit: &Hit, lo: [f64; 3], hi: [f64; 3]) -> [i32; 3] {
    let mut a = [0.0; 3];
    for k in 0..3 {
        a[k] = if k == hit.axis {
            if hit.positive {
                hit.point[k] - lo[k]
            } else {
                hit.point[k] - hi[k]
            }
        } else {
            hit.point[k] - (lo[k] + hi[k]) / 2.0
        };
    }
    a.map(|v| (v * 10.0).round() as i32)
}

/// Name and port for a new device: the next free port and a unique name.
pub fn default_device(model: DeviceModel, existing: &[DeviceInstance]) -> DeviceInstance {
    if model == DeviceModel::Hub {
        return DeviceInstance {
            name: unique_name("hub", existing),
            port: None,
        };
    }
    let base = match model {
        DeviceModel::Hub => "hub",
        DeviceModel::MotorMedium | DeviceModel::MotorLarge => "motor",
        DeviceModel::Servo => "servo",
        DeviceModel::Ultrasonic => "ultrasonic",
        DeviceModel::TimeOfFlight => "tof",
        DeviceModel::Color => "color",
        DeviceModel::Touch => "touch",
    };
    let port = Port::ALL
        .into_iter()
        .find(|p| !existing.iter().any(|d| d.port == Some(*p)))
        .unwrap_or(Port::F);
    DeviceInstance {
        name: unique_name(base, existing),
        port: Some(port),
    }
}

fn unique_name(base: &str, existing: &[DeviceInstance]) -> String {
    if !existing.iter().any(|d| d.name == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !existing.iter().any(|d| &d.name == c))
        .unwrap_or_else(|| base.to_string())
}

/// The category of a part, for the palette.
pub fn category_of(cat: &Catalog, id: &str) -> Option<Category> {
    cat.get(id).map(|d| d.category)
}

/// Derive connections without a builder (a convenience for tests and tools).
pub fn connections_of(cat: &Catalog, asm: &Assembly) -> Vec<Connection> {
    derive_connections(cat, asm)
}
