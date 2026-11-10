//! FSRS-4.5, the Free Spaced Repetition Scheduler, with its published default weights.
//!
//! A card has a *stability* S (days until recall falls to 90%) and a *difficulty* D (1 to 10). The
//! chance of recalling it after `t` days is `R = (1 + F t / S)^-0.5` with `F = 19/81`. A review
//! with grade 1 (again), 2 (hard), 3 (good) or 4 (easy) updates both numbers. The schedule asks
//! for the next review when `R` has fallen to the wanted retention.

use serde::{Deserialize, Serialize};

/// The 17 default weights of FSRS-4.5 (the published ones; the owner confirmed them, with a
/// target retention of 0.9, in decision 0016).
pub const W: [f64; 17] = [
    0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367, 1.0461, 2.1072,
    0.0793, 0.3246, 1.587, 0.2272, 2.8755,
];
/// Exponent of the forgetting curve.
const DECAY: f64 = -0.5;
/// Factor of the forgetting curve, so that `R = 0.9` when `t = S`.
const FACTOR: f64 = 19.0 / 81.0;
/// The retention the schedule aims for.
pub const WANTED_RETENTION: f64 = 0.9;

/// How well a review went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grade {
    /// Forgotten.
    Again = 1,
    /// Recalled with effort.
    Hard = 2,
    /// Recalled.
    Good = 3,
    /// Recalled easily.
    Easy = 4,
}

/// The memory state of one idea.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// Stability, days.
    pub stability: f64,
    /// Difficulty, 1 to 10.
    pub difficulty: f64,
    /// Day of the last review (days since 1970-01-01, fractional allowed).
    pub last_day: f64,
    /// Reviews so far.
    pub reps: u32,
    /// Times it was forgotten.
    pub lapses: u32,
}

fn clamp_d(d: f64) -> f64 {
    d.clamp(1.0, 10.0)
}

fn init_difficulty(g: Grade) -> f64 {
    clamp_d(W[4] - (f64::from(g as u8) - 3.0) * W[5])
}

/// Probability of recalling after `elapsed_days` with stability `s`.
pub fn retrievability(elapsed_days: f64, s: f64) -> f64 {
    (1.0 + FACTOR * elapsed_days.max(0.0) / s).powf(DECAY)
}

/// Days until the recall probability falls to `r` for stability `s`.
pub fn interval_days(s: f64, r: f64) -> f64 {
    s / FACTOR * (r.powf(1.0 / DECAY) - 1.0)
}

impl Card {
    /// A card after its first review on `day`.
    pub fn first(g: Grade, day: f64) -> Self {
        Self {
            stability: W[g as usize - 1].max(0.1),
            difficulty: init_difficulty(g),
            last_day: day,
            reps: 1,
            lapses: u32::from(g == Grade::Again),
        }
    }

    /// Recall probability on `day`.
    pub fn recall(&self, day: f64) -> f64 {
        retrievability(day - self.last_day, self.stability)
    }

    /// The day the card falls due (recall at the wanted retention).
    pub fn due_day(&self) -> f64 {
        self.last_day + interval_days(self.stability, WANTED_RETENTION)
    }

    /// Review on `day`.
    pub fn review(&self, g: Grade, day: f64) -> Self {
        let r = self.recall(day);
        let (s, d) = (self.stability, self.difficulty);
        let new_d = clamp_d(
            W[7] * init_difficulty(Grade::Easy)
                + (1.0 - W[7]) * (d - W[6] * (f64::from(g as u8) - 3.0)),
        );
        let new_s = if g == Grade::Again {
            W[11] * d.powf(-W[12]) * ((s + 1.0).powf(W[13]) - 1.0) * (W[14] * (1.0 - r)).exp()
        } else {
            let hard = if g == Grade::Hard { W[15] } else { 1.0 };
            let easy = if g == Grade::Easy { W[16] } else { 1.0 };
            s * (W[8].exp()
                * (11.0 - d)
                * s.powf(-W[9])
                * (((1.0 - r) * W[10]).exp() - 1.0)
                * hard
                * easy
                + 1.0)
        };
        Self {
            stability: new_s.max(0.01),
            difficulty: new_d,
            last_day: day,
            reps: self.reps + 1,
            lapses: self.lapses + u32::from(g == Grade::Again),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parameters_are_the_published_fsrs_4_5_defaults_with_ninety_percent_retention() {
        // Confirmed by the owner (decision 0016): FSRS-4.5, default weights, target retention 0.9.
        assert_eq!(
            W,
            [
                0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367,
                1.0461, 2.1072, 0.0793, 0.3246, 1.587, 0.2272, 2.8755
            ]
        );
        assert_eq!(WANTED_RETENTION, 0.9);
    }

    #[test]
    fn recall_is_ninety_percent_after_one_stability() {
        let r = retrievability(7.0, 7.0);
        assert!((r - 0.9).abs() < 1e-12, "{r}");
        assert!((interval_days(7.0, 0.9) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn first_review_uses_the_published_weights() {
        let c = Card::first(Grade::Good, 100.0);
        assert!((c.stability - 3.7145).abs() < 1e-12);
        // D0(3) = w4.
        assert!((c.difficulty - 5.1618).abs() < 1e-12);
        assert!((Card::first(Grade::Again, 0.0).stability - 0.4872).abs() < 1e-12);
        assert!((Card::first(Grade::Easy, 0.0).stability - 13.8206).abs() < 1e-12);
        assert!(
            Card::first(Grade::Easy, 0.0).difficulty < Card::first(Grade::Again, 0.0).difficulty
        );
    }

    #[test]
    fn a_good_review_at_the_due_day_grows_stability_and_a_lapse_shrinks_it() {
        let c = Card::first(Grade::Good, 0.0);
        let due = c.due_day();
        assert!(
            (due - 3.7145).abs() < 1e-9,
            "due when R = 0.9, i.e. after S days: {due}"
        );
        let good = c.review(Grade::Good, due);
        assert!(
            good.stability > c.stability * 1.5,
            "{} vs {}",
            good.stability,
            c.stability
        );
        let easy = c.review(Grade::Easy, due);
        let hard = c.review(Grade::Hard, due);
        assert!(easy.stability > good.stability && good.stability > hard.stability);
        let lapse = c.review(Grade::Again, due);
        assert!(lapse.stability < c.stability);
        assert_eq!(lapse.lapses, 1);
        assert!(lapse.difficulty > c.difficulty);
    }

    #[test]
    fn reviewing_later_gains_more() {
        let c = Card::first(Grade::Good, 0.0);
        assert!(c.review(Grade::Good, 20.0).stability > c.review(Grade::Good, 4.0).stability);
    }

    #[test]
    fn difficulty_stays_in_range() {
        let mut c = Card::first(Grade::Good, 0.0);
        for i in 0..50 {
            c = c.review(Grade::Again, f64::from(i));
        }
        assert!((1.0..=10.0).contains(&c.difficulty));
    }
}
