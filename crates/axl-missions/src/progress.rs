//! What the learner has done: best stars, attempts, hints, predictions and the memory state of
//! each idea (for the warm-ups), saved as RON in `progress.ron`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::learn::fsrs::{Card, Grade};

/// Current `version` of `progress.ron`.
pub const PROGRESS_VERSION: u32 = 1;

/// Progress on one mission.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MissionProgress {
    /// Best stars (0 to 3).
    pub stars: u8,
    /// Graded runs.
    pub attempts: u32,
    /// Graded runs that failed.
    #[serde(default)]
    pub failed_runs: u32,
    /// A hint was opened on the best run.
    pub hint_used: bool,
    /// Hints opened so far in the current attempt series (0 to 3).
    pub hints_open: u8,
    /// Day of the first pass (days since 1970-01-01); the date shown is derived from it.
    #[serde(default)]
    pub completed_day: Option<f64>,
    /// The answer given to the prediction question before the first run.
    #[serde(default)]
    pub predicted: Option<f64>,
}

/// All progress.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// File format version.
    #[serde(default = "version_one")]
    pub version: u32,
    /// By mission id.
    pub missions: BTreeMap<String, MissionProgress>,
    /// Memory state per idea (the first concept of a mission), for the warm-ups.
    #[serde(default)]
    pub cards: BTreeMap<String, Card>,
}

fn version_one() -> u32 {
    PROGRESS_VERSION
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            version: PROGRESS_VERSION,
            missions: BTreeMap::new(),
            cards: BTreeMap::new(),
        }
    }
}

impl Progress {
    /// Load; missing or unreadable files, and files from a newer AXL, give an empty record (the
    /// old file is left alone until the next save, which keeps it as `progress.ron.bak`).
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| ron::from_str::<Self>(&t).ok())
            .filter(|p| p.version <= PROGRESS_VERSION)
            .unwrap_or_default()
    }

    /// Save atomically (a temporary file in the same folder, then a rename), keeping the previous
    /// copy as `<file>.bak`.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        let tmp = path.with_extension("ron.tmp");
        std::fs::write(&tmp, text)?;
        if path.exists() {
            let mut bak = path.as_os_str().to_owned();
            bak.push(".bak");
            let _ = std::fs::copy(path, bak);
        }
        std::fs::rename(&tmp, path)
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

    /// Record a graded run with its outcome. `first_concept` is the idea the mission teaches: its
    /// memory card starts (or is reviewed as good) the first time the mission is passed.
    pub fn record_run(
        &mut self,
        id: &str,
        passed: bool,
        stars: u8,
        hint_used: bool,
        first_concept: Option<&str>,
        day: f64,
    ) {
        self.record(id, stars, hint_used);
        let p = self.missions.entry(id.to_owned()).or_default();
        if passed {
            if p.completed_day.is_none() {
                p.completed_day = Some(day);
                if let Some(c) = first_concept {
                    self.cards
                        .entry(c.to_owned())
                        .or_insert_with(|| Card::first(Grade::Good, day));
                }
            }
        } else {
            p.failed_runs += 1;
        }
    }

    /// Stars on a mission.
    pub fn stars(&self, id: &str) -> u8 {
        self.missions.get(id).map_or(0, |p| p.stars)
    }

    /// Has the mission been passed?
    pub fn completed(&self, id: &str) -> bool {
        self.missions
            .get(id)
            .is_some_and(|p| p.completed_day.is_some())
    }

    /// Hints are unlocked by the first failed run.
    pub fn hints_unlocked(&self, id: &str) -> bool {
        self.missions.get(id).is_some_and(|p| p.failed_runs > 0)
    }

    /// Open the next hint (0, 1 or 2) if hints are unlocked and some are left. Returns its index.
    pub fn open_next_hint(&mut self, id: &str) -> Option<u8> {
        if !self.hints_unlocked(id) {
            return None;
        }
        let p = self.missions.get_mut(id)?;
        if p.hints_open >= 3 {
            return None;
        }
        p.hints_open += 1;
        Some(p.hints_open - 1)
    }

    /// Hints opened on a mission.
    pub fn hints_open(&self, id: &str) -> u8 {
        self.missions.get(id).map_or(0, |p| p.hints_open)
    }

    /// Remember the prediction made before the first run.
    pub fn set_prediction(&mut self, id: &str, value: f64) {
        self.missions.entry(id.to_owned()).or_default().predicted = Some(value);
    }

    /// Review an idea after a warm-up question.
    pub fn review_concept(&mut self, concept: &str, correct: bool, day: f64) {
        let grade = if correct { Grade::Good } else { Grade::Again };
        let next = match self.cards.get(concept) {
            Some(c) => c.review(grade, day),
            None => Card::first(grade, day),
        };
        self.cards.insert(concept.to_owned(), next);
    }
}

/// Days since 1970-01-01 for a `SystemTime`.
pub fn day_of(t: std::time::SystemTime) -> f64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() / 86_400.0)
}

/// The civil date (year, month, day) of a day number, for showing a completion date.
pub fn civil_date(day: f64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil algorithm.
    let z = day.floor() as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("axl-{name}-{}.ron", std::process::id()))
    }

    #[test]
    fn record_keeps_best_and_round_trips() {
        let mut p = Progress::default();
        p.record("m01", 2, false);
        p.record("m01", 1, true);
        assert_eq!(p.stars("m01"), 2);
        assert_eq!(p.missions["m01"].attempts, 2);
        let f = temp("prog");
        p.save(&f).unwrap();
        assert_eq!(Progress::load(&f), p);
        let _ = std::fs::remove_file(&f);
        assert_eq!(Progress::load(&f), Progress::default());
    }

    #[test]
    fn hints_unlock_after_the_first_failed_run_and_open_in_order() {
        let mut p = Progress::default();
        assert!(!p.hints_unlocked("m01"));
        assert_eq!(p.open_next_hint("m01"), None);
        p.record_run("m01", false, 0, false, Some("Actuators"), 100.0);
        assert!(p.hints_unlocked("m01"));
        assert_eq!(p.open_next_hint("m01"), Some(0));
        assert_eq!(p.open_next_hint("m01"), Some(1));
        assert_eq!(p.open_next_hint("m01"), Some(2));
        assert_eq!(p.open_next_hint("m01"), None);
        assert_eq!(p.hints_open("m01"), 3);
        // Other missions are unaffected.
        assert!(!p.hints_unlocked("m02"));
    }

    #[test]
    fn the_first_pass_sets_the_date_and_starts_the_memory_card_once() {
        let mut p = Progress::default();
        p.record_run("m01", true, 1, false, Some("Actuators"), 20_000.0);
        p.record_run("m01", true, 2, false, Some("Actuators"), 20_005.0);
        assert!(p.completed("m01"));
        assert_eq!(p.missions["m01"].completed_day, Some(20_000.0));
        assert_eq!(p.cards["Actuators"].last_day, 20_000.0);
        assert_eq!(p.cards["Actuators"].reps, 1);
        assert_eq!(p.stars("m01"), 2);
    }

    #[test]
    fn warm_ups_review_the_card() {
        let mut p = Progress::default();
        p.review_concept("Noise", true, 10.0);
        let s1 = p.cards["Noise"].stability;
        p.review_concept("Noise", true, 14.0);
        assert!(p.cards["Noise"].stability > s1);
        p.review_concept("Noise", false, 30.0);
        assert_eq!(p.cards["Noise"].lapses, 1);
    }

    #[test]
    fn save_is_atomic_keeps_a_backup_and_a_newer_file_is_not_loaded() {
        let f = temp("prog2");
        let mut p = Progress::default();
        p.set_prediction("m01", 480.0);
        p.save(&f).unwrap();
        p.record("m01", 1, false);
        p.save(&f).unwrap();
        let mut bak = f.as_os_str().to_owned();
        bak.push(".bak");
        let bak = std::path::PathBuf::from(bak);
        assert_eq!(Progress::load(&bak).stars("m01"), 0);
        assert_eq!(Progress::load(&f).stars("m01"), 1);
        assert_eq!(Progress::load(&f).missions["m01"].predicted, Some(480.0));
        std::fs::write(&f, "(version: 99, missions: {})").unwrap();
        assert_eq!(Progress::load(&f), Progress::default());
        // Old files without a version or cards still load.
        std::fs::write(
            &f,
            "(missions: {\"m01\": (stars: 3, attempts: 2, hint_used: false, hints_open: 0)})",
        )
        .unwrap();
        assert_eq!(Progress::load(&f).stars("m01"), 3);
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(&bak);
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0.0), (1970, 1, 1));
        assert_eq!(civil_date(20_000.5), (2024, 10, 4));
        assert_eq!(civil_date(11_016.0), (2000, 2, 29));
    }
}
