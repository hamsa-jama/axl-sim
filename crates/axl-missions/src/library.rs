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

/// The name of a chapter of the curriculum.
pub fn chapter_title(chapter: u8) -> &'static str {
    match chapter {
        1 => "Chapter 1: Make it move",
        2 => "Chapter 2: Sense the world",
        3 => "Chapter 3: Close the loop",
        4 => "Chapter 4: Think and build",
        _ => "More missions",
    }
}

/// The missions grouped by chapter, in curriculum order.
pub fn chapters() -> Result<Vec<(u8, Vec<MissionDef>)>, MissionError> {
    let mut out: Vec<(u8, Vec<MissionDef>)> = Vec::new();
    for m in builtin_missions()? {
        match out.last_mut() {
            Some((c, v)) if *c == m.chapter => v.push(m),
            _ => out.push((m.chapter, vec![m])),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_missions_in_four_named_chapters() {
        let ch = chapters().unwrap();
        assert_eq!(ch.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(ch.iter().map(|(_, v)| v.len()).sum::<usize>(), 12);
        for (c, _) in &ch {
            assert!(chapter_title(*c).starts_with("Chapter"));
        }
    }
}
