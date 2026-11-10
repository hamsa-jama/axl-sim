//! The built-in missions.

use crate::def::MissionDef;

/// A mission could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum MissionError {
    /// Bad RON.
    #[error("mission {id}: {message}")]
    Parse {
        /// File.
        id: String,
        /// What is wrong.
        message: String,
    },
    /// Unknown id.
    #[error("no mission {0}")]
    Unknown(String),
}

macro_rules! missions {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_str!(concat!("../../../assets/missions/", $id, ".ron")))),*]
    };
}

const FILES: &[(&str, &str)] = missions!(
    "m01", "m02", "m03", "m04", "m05", "m06", "m07", "m08", "m09", "m10", "m11", "m12"
);

/// All missions, in curriculum order.
pub fn builtin_missions() -> Result<Vec<MissionDef>, MissionError> {
    let mut v = Vec::new();
    for (id, text) in FILES {
        v.push(
            MissionDef::from_ron(text).map_err(|message| MissionError::Parse {
                id: (*id).to_owned(),
                message,
            })?,
        );
    }
    Ok(v)
}

/// One mission by id.
pub fn mission(id: &str) -> Result<MissionDef, MissionError> {
    builtin_missions()?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| MissionError::Unknown(id.to_owned()))
}
