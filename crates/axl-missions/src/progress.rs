//! What the learner has done: best stars, attempts and hint use per mission, saved as RON.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Progress on one mission.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MissionProgress {
    /// Best stars (0 to 3).
    pub stars: u8,
    /// Graded runs.
    pub attempts: u32,
    /// A hint was opened on the best run.
    pub hint_used: bool,
    /// Hints opened so far in the current attempt series (0 to 3).
    pub hints_open: u8,
}

/// All progress.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// By mission id.
    pub missions: BTreeMap<String, MissionProgress>,
}

impl Progress {
    /// Load; missing or unreadable files give an empty record.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| ron::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Save.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Record a graded run.
    pub fn record(&mut self, id: &str, stars: u8, hint_used: bool) {
        let p = self.missions.entry(id.to_owned()).or_default();
        p.attempts += 1;
        if stars > p.stars {
            p.stars = stars;
            p.hint_used = hint_used;
        }
    }

    /// Stars on a mission.
    pub fn stars(&self, id: &str) -> u8 {
        self.missions.get(id).map_or(0, |p| p.stars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_keeps_best_and_round_trips() {
        let mut p = Progress::default();
        p.record("m01", 2, false);
        p.record("m01", 1, true);
        assert_eq!(p.stars("m01"), 2);
        assert_eq!(p.missions["m01"].attempts, 2);
        let f = std::env::temp_dir().join(format!("axl-prog-{}.ron", std::process::id()));
        p.save(&f).unwrap();
        assert_eq!(Progress::load(&f), p);
        let _ = std::fs::remove_file(&f);
        assert_eq!(Progress::load(&f), Progress::default());
    }
}
