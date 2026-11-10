//! The worlds and objects that ship with AXL.

use std::collections::BTreeMap;

use crate::def::WorldDef;
use crate::objects::ObjectDef;

/// A world or object file that does not parse.
#[derive(Debug, thiserror::Error)]
#[error("{file}: {message}")]
pub struct WorldError {
    /// Which file.
    pub file: String,
    /// What is wrong.
    pub message: String,
}

macro_rules! worlds {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_str!(concat!("../../../assets/worlds/", $id, ".ron")))),*]
    };
}
macro_rules! objects {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_str!(concat!("../../../assets/objects/", $id, ".ron")))),*]
    };
}

const WORLD_FILES: &[(&str, &str)] = worlds!(
    "open_grid",
    "test_track",
    "line_track",
    "walled_arena",
    "maze",
    "ramp_lab"
);
const OBJECT_FILES: &[(&str, &str)] = objects!(
    "cube_wood_40",
    "cube_steel_40",
    "box_card_100",
    "cylinder_wood_40x60",
    "ball_table_tennis",
    "ball_tennis",
    "can_330"
);

/// Every shipped world, by id.
pub fn builtin_worlds() -> Result<BTreeMap<String, WorldDef>, WorldError> {
    let mut out = BTreeMap::new();
    for (id, text) in WORLD_FILES {
        let w = WorldDef::from_ron(text).map_err(|message| WorldError {
            file: format!("assets/worlds/{id}.ron"),
            message,
        })?;
        if w.id != *id {
            return Err(WorldError {
                file: format!("assets/worlds/{id}.ron"),
                message: format!("the id is {:?}, not the file name", w.id),
            });
        }
        out.insert(w.id.clone(), w);
    }
    Ok(out)
}

/// Every shipped object, by id.
pub fn builtin_objects() -> Result<BTreeMap<String, ObjectDef>, WorldError> {
    let mut out = BTreeMap::new();
    for (id, text) in OBJECT_FILES {
        let o = ObjectDef::from_ron(text).map_err(|message| WorldError {
            file: format!("assets/objects/{id}.ron"),
            message,
        })?;
        if o.id != *id {
            return Err(WorldError {
                file: format!("assets/objects/{id}.ron"),
                message: format!("the id is {:?}, not the file name", o.id),
            });
        }
        out.insert(o.id.clone(), o);
    }
    Ok(out)
}

/// One shipped world.
pub fn world(id: &str) -> Result<WorldDef, WorldError> {
    builtin_worlds()?.remove(id).ok_or_else(|| WorldError {
        file: id.to_owned(),
        message: "there is no such world".to_owned(),
    })
}
