//! The validation panel's checks (SPEC.md, "Validation"). Errors block Simulate; warnings do not.

use std::collections::{BTreeMap, BTreeSet};

use axl_parts::catalog::Catalog;
use axl_parts::types::{Category, ConnKind};

use crate::geom::{ConnIndex, overlapping_pairs};
use crate::model::{Assembly, Connection, Port};

/// How serious an issue is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Blocks Simulate.
    Error,
    /// Does not block Simulate.
    Warning,
}

/// One line of the validation panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Error or warning.
    pub level: Level,
    /// The message shown to the learner.
    pub message: String,
    /// Parts to highlight.
    pub parts: Vec<u32>,
}

/// Run every check.
pub fn validate(cat: &Catalog, asm: &Assembly, idx: &ConnIndex, conns: &[Connection]) -> Vec<Issue> {
    let mut out = Vec::new();
    let def_of = |uid: u32| asm.get(uid).and_then(|p| cat.get(&p.part.0));
    let hubs: Vec<u32> = asm
        .parts
        .iter()
        .filter(|p| def_of(p.uid).is_some_and(|d| d.category == Category::Hub))
        .map(|p| p.uid)
        .collect();
    if hubs.len() != 1 {
        out.push(Issue {
            level: Level::Error,
            message: "A robot needs exactly one hub.".into(),
            parts: hubs.clone(),
        });
    }

    let devices: Vec<u32> = asm
        .parts
        .iter()
        .filter(|p| p.device.is_some() && !hubs.contains(&p.uid))
        .map(|p| p.uid)
        .collect();
    if devices.len() > 6 {
        out.push(Issue {
            level: Level::Error,
            message: "The hub has six ports. Remove a motor or sensor.".into(),
            parts: devices.clone(),
        });
    }

    // Floating parts: everything outside the component that holds the hub (or the largest one).
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = asm.parts.iter().map(|p| (p.uid, BTreeSet::new())).collect();
    for c in conns {
        adj.entry(c.a.uid).or_default().insert(c.b.uid);
        adj.entry(c.b.uid).or_default().insert(c.a.uid);
    }
    let mut comps: Vec<Vec<u32>> = Vec::new();
    let mut seen = BTreeSet::new();
    for &start in adj.keys() {
        if seen.contains(&start) {
            continue;
        }
        let mut comp = vec![];
        let mut stack = vec![start];
        seen.insert(start);
        while let Some(u) = stack.pop() {
            comp.push(u);
            for &v in &adj[&u] {
                if seen.insert(v) {
                    stack.push(v);
                }
            }
        }
        comp.sort();
        comps.push(comp);
    }
    if comps.len() > 1 {
        let main = if hubs.len() == 1 {
            comps.iter().position(|c| c.contains(&hubs[0]))
        } else {
            None
        }
        .unwrap_or_else(|| {
            // Largest component; the first (lowest uid) wins ties.
            let mut best = 0;
            for (i, c) in comps.iter().enumerate() {
                if c.len() > comps[best].len() {
                    best = i;
                }
            }
            best
        });
        let floating: Vec<u32> = comps
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != main)
            .flat_map(|(_, c)| c.iter().copied())
            .collect();
        let n = floating.len();
        out.push(Issue {
            level: Level::Error,
            message: format!(
                "{n} {} floating. Connect them or delete them.",
                if n == 1 { "part is" } else { "parts are" }
            ),
            parts: floating,
        });
    }

    // Names and ports.
    let mut by_name: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    let mut by_port: BTreeMap<Port, Vec<u32>> = BTreeMap::new();
    for p in &asm.parts {
        if let Some(d) = &p.device {
            by_name.entry(d.name.as_str()).or_default().push(p.uid);
            if let Some(port) = d.port {
                by_port.entry(port).or_default().push(p.uid);
            }
        }
    }
    for (name, uids) in by_name.iter().filter(|(_, v)| v.len() > 1) {
        out.push(Issue {
            level: Level::Error,
            message: format!("Two devices are called {name}."),
            parts: uids.clone(),
        });
    }
    for (port, uids) in by_port.iter().filter(|(_, v)| v.len() > 1) {
        out.push(Issue {
            level: Level::Error,
            message: format!("Two devices use port {}.", port.letter()),
            parts: uids.clone(),
        });
    }

    // Overlaps.
    for (a, b) in overlapping_pairs(cat, asm, idx, conns) {
        let name = |u: u32| def_of(u).map(|d| d.name.clone()).unwrap_or_default();
        out.push(Issue {
            level: Level::Error,
            message: format!("{} and {} overlap.", name(a), name(b)),
            parts: vec![a, b],
        });
    }

    // Warnings: motors that turn nothing, wheels that cannot turn.
    let connected: BTreeSet<_> = conns.iter().flat_map(|c| [c.a, c.b]).collect();
    for p in &asm.parts {
        let Some(def) = cat.get(&p.part.0) else { continue };
        match def.category {
            Category::Motor if def.bodies.len() == 2 => {
                let rotor_used = idx
                    .conns
                    .iter()
                    .any(|w| w.r.uid == p.uid && w.r.body == 1 && connected.contains(&w.r));
                if !rotor_used {
                    let label = p
                        .device
                        .as_ref()
                        .and_then(|d| d.port)
                        .map(|port| format!("Motor {}", port.letter()))
                        .unwrap_or_else(|| "This motor".into());
                    out.push(Issue {
                        level: Level::Warning,
                        message: format!("{label} turns nothing."),
                        parts: vec![p.uid],
                    });
                }
            }
            Category::Wheel => {
                let on_axle = conns.iter().any(|c| {
                    let other = if c.a.uid == p.uid {
                        Some(c.b)
                    } else if c.b.uid == p.uid {
                        Some(c.a)
                    } else {
                        None
                    };
                    other.is_some_and(|o| {
                        idx.conns.iter().any(|w| w.r == o && matches!(w.kind, ConnKind::AxleShaft { .. }))
                    })
                });
                if !on_axle {
                    out.push(Issue {
                        level: Level::Warning,
                        message: "This wheel can't turn.".into(),
                        parts: vec![p.uid],
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// True when any issue is an error.
pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(|i| i.level == Level::Error)
}
