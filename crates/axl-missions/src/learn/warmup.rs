//! Warm-up questions: one generator per idea, with numbers drawn from a seed, and the choice of
//! which ideas are due.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::fsrs::Card;

/// A question with a number as its answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    /// The idea it asks about (the first concept of a mission, for example `Wheel kinematics`).
    pub concept: String,
    /// The question.
    pub text: String,
    /// Unit of the answer, shown beside the box.
    pub unit: String,
    /// The right answer.
    pub answer: f64,
    /// Relative tolerance (fraction of the answer).
    pub tol_rel: f64,
    /// Absolute tolerance, in the unit of the answer.
    pub tol_abs: f64,
    /// One line that shows the working, shown after the answer.
    pub working: String,
}

impl Question {
    /// Is `given` close enough?
    pub fn is_correct(&self, given: f64) -> bool {
        given.is_finite()
            && (given - self.answer).abs() <= self.tol_abs.max(self.tol_rel * self.answer.abs())
    }
}

/// A small deterministic generator (SplitMix64): numbers for a question depend only on the seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next() % items.len() as u64) as usize]
    }
}

/// The ideas that have a generator, in curriculum order.
pub const CONCEPTS: [&str; 12] = [
    "Actuators",
    "Wheel kinematics",
    "Differential drive",
    "Odometry",
    "Range sensors",
    "Noise",
    "Feedback control",
    "On-off control",
    "PID tuning",
    "Gyro",
    "State machines",
    "Gear ratios",
];

fn q(
    concept: &str,
    text: String,
    unit: &str,
    answer: f64,
    tol_rel: f64,
    tol_abs: f64,
    working: String,
) -> Question {
    Question {
        concept: concept.to_owned(),
        text,
        unit: unit.to_owned(),
        answer,
        tol_rel,
        tol_abs,
        working,
    }
}

/// A question about `concept` with numbers drawn from `seed`. The same seed gives the same
/// question. `None` for an idea without a generator.
pub fn generate(concept: &str, seed: u64) -> Option<Question> {
    let mut r = Rng(seed ^ 0xA5A5_5A5A_1234_5678);
    r.next();
    Some(match concept {
        "Actuators" => {
            let v = r.pick(&[7.4_f64, 7.8, 8.0, 8.4]);
            let p = r.pick(&[0.25_f64, 0.4, 0.5, 0.6, 0.75]);
            q(
                concept,
                format!(
                    "The battery reads {v} V. A motor is driven at power {p}. How many volts does the motor get?"
                ),
                "V",
                v * p,
                0.0,
                0.05,
                format!(
                    "Power is a fraction of the battery voltage: {p} x {v} = {:.2} V.",
                    v * p
                ),
            )
        }
        "Wheel kinematics" => {
            let d = r.pick(&[30.0_f64, 43.0, 56.0, 81.0]);
            let n = r.pick(&[2.0_f64, 3.0, 4.0, 5.0, 6.0]);
            let a = PI * d * n;
            q(
                concept,
                format!(
                    "A {d} mm wheel turns {n} times without slipping. How far does the robot roll?"
                ),
                "mm",
                a,
                0.01,
                0.5,
                format!(
                    "One turn rolls the circumference, pi x {d} = {:.1} mm; {n} turns give {a:.1} mm.",
                    PI * d
                ),
            )
        }
        "Differential drive" => {
            let b = r.pick(&[120.0_f64, 145.0, 160.0, 180.0]);
            let deg = r.pick(&[45.0_f64, 90.0, 180.0]);
            let a = b / 2.0 * deg.to_radians();
            q(
                concept,
                format!(
                    "A robot's wheels are {b} mm apart. To turn on the spot by {deg} degrees, how far must each wheel roll (one forwards, one backwards)?"
                ),
                "mm",
                a,
                0.01,
                0.5,
                format!(
                    "Each wheel moves on a circle of radius {} mm: arc = radius x angle = {} x {:.3} rad = {a:.1} mm.",
                    b / 2.0,
                    b / 2.0,
                    deg.to_radians()
                ),
            )
        }
        "Odometry" => {
            let dist = r.pick(&[1.0_f64, 1.5, 2.0, 3.0]);
            let deg = r.pick(&[1.0_f64, 2.0, 3.0, 5.0]);
            let a = dist * 1000.0 * deg.to_radians().sin();
            q(
                concept,
                format!(
                    "A robot drives {dist} m with its heading wrong by {deg} degrees from the start. How far to the side of the intended line does it end up?"
                ),
                "mm",
                a,
                0.03,
                1.0,
                format!(
                    "Sideways error = distance x sin(heading error) = {} mm x sin({deg} deg) = {a:.1} mm.",
                    dist * 1000.0
                ),
            )
        }
        "Range sensors" => {
            let t = r.pick(&[2.0_f64, 3.0, 4.0, 5.0, 8.0, 10.0]);
            let a = 343.0 * (t / 1000.0) / 2.0 * 1000.0;
            q(
                concept,
                format!(
                    "An ultrasonic sensor hears its echo {t} ms after the pulse. Sound travels at 343 m/s. How far away is the wall?"
                ),
                "mm",
                a,
                0.01,
                1.0,
                format!(
                    "The pulse goes there and back, so distance = speed x time / 2 = 343 x {} / 2 = {a:.1} mm.",
                    t / 1000.0
                ),
            )
        }
        "Noise" => {
            let s = r.pick(&[3.0_f64, 4.0, 6.0, 8.0]);
            let n = r.pick(&[4.0_f64, 9.0, 16.0, 25.0]);
            let a = s / n.sqrt();
            q(
                concept,
                format!(
                    "One distance reading has a standard deviation of {s} mm. You average {n} independent readings. What is the standard deviation of the average?"
                ),
                "mm",
                a,
                0.02,
                0.02,
                format!(
                    "Averaging n readings divides the spread by the square root of n: {s} / sqrt({n}) = {a:.2} mm."
                ),
            )
        }
        "Feedback control" => {
            let g = r.pick(&[0.2_f64, 0.25, 0.3, 0.4]);
            let m = r.pick(&[0.05_f64, 0.1, 0.15, 0.18]);
            let k = r.pick(&[1.0_f64, 1.5, 2.0, 3.0]);
            let a = k * (g - m);
            q(
                concept,
                format!(
                    "A P controller wants {g} m/s and measures {m} m/s. Its gain is {k}. What is its output (the correction)?"
                ),
                "",
                a,
                0.02,
                0.005,
                format!("Output = gain x error = {k} x ({g} - {m}) = {a:.3}."),
            )
        }
        "On-off control" => {
            let refl = r.pick(&[10.0_f64, 20.0, 60.0, 70.0, 80.0]);
            let k = r.pick(&[0.2_f64, 0.35, 0.5]);
            let a = k * (refl - 40.0) / 40.0;
            q(
                concept,
                format!(
                    "A line follower reads reflected light {refl}; the edge of the line is at 40. Its error is (reading - 40) / 40 and its turn is gain x error with gain {k}. What is the turn?"
                ),
                "",
                a,
                0.02,
                0.005,
                format!(
                    "error = ({refl} - 40) / 40 = {:.3}; turn = {k} x {:.3} = {a:.3}.",
                    (refl - 40.0) / 40.0,
                    (refl - 40.0) / 40.0
                ),
            )
        }
        "PID tuning" => {
            let e0 = r.pick(&[0.04_f64, 0.06, 0.1]);
            let e1 = r.pick(&[0.01_f64, 0.02, 0.03]);
            let dt = r.pick(&[0.01_f64, 0.02, 0.05]);
            let kd = r.pick(&[0.02_f64, 0.05, 0.1]);
            let a = kd * (e1 - e0) / dt;
            q(
                concept,
                format!(
                    "The error was {e0} and is now {e1} after {dt} s. With kd = {kd}, what is the D term (kd x change in error / time)?"
                ),
                "",
                a,
                0.02,
                0.005,
                format!(
                    "D = {kd} x ({e1} - {e0}) / {dt} = {a:.3} (negative: the error is shrinking, so the D term brakes the correction)."
                ),
            )
        }
        "Gyro" => {
            let w = r.pick(&[0.5_f64, 1.0, 1.5, 2.0]);
            let b = r.pick(&[0.005_f64, 0.01, 0.02]);
            let t = r.pick(&[2.0_f64, 3.0, 5.0, 10.0]);
            let a = ((w - b) * t).to_degrees();
            q(
                concept,
                format!(
                    "A gyro reads {w} rad/s on average for {t} s, and its bias is {b} rad/s. By how many degrees did the robot really turn?"
                ),
                "deg",
                a,
                0.01,
                0.5,
                format!(
                    "Subtract the bias, then add up rate x time: ({w} - {b}) x {t} = {:.3} rad = {a:.1} deg.",
                    (w - b) * t
                ),
            )
        }
        "State machines" => {
            let n = r.pick(&[8.0_f64, 10.0, 12.0, 14.0]);
            let k = r.pick(&[2.0_f64, 4.0, 6.0]);
            let t = r.pick(&[3.0_f64, 4.0, 5.0]);
            let a = (n + k) * t;
            q(
                concept,
                format!(
                    "A maze robot takes {t} s for every cell. The shortest way out is {n} cells, but it wastes {k} extra cells on dead ends. How long does it take?"
                ),
                "s",
                a,
                0.0,
                0.5,
                format!(
                    "Cells travelled = {n} + {k} = {}; time = {} x {t} = {a:.0} s.",
                    n + k,
                    n + k
                ),
            )
        }
        "Gear ratios" => {
            let a = r.pick(&[8.0_f64, 12.0, 16.0, 20.0]);
            let b = r.pick(&[24.0_f64, 36.0, 40.0]);
            let rpm = r.pick(&[120.0_f64, 180.0, 240.0]);
            let out = rpm * a / b;
            q(
                concept,
                format!(
                    "A {a}-tooth gear on the motor drives a {b}-tooth gear on the axle. The motor turns at {rpm} rpm. How fast does the axle turn?"
                ),
                "rpm",
                out,
                0.01,
                0.1,
                format!(
                    "Output speed = input speed x driver teeth / driven teeth = {rpm} x {a} / {b} = {out:.1} rpm (slower, but with {:.2} times the torque).",
                    b / a
                ),
            )
        }
        _ => return None,
    })
}

/// The ideas to ask about before a mission: those whose cards are due on `day` among
/// `candidates` (the first concepts of the earlier missions), the least remembered first, at
/// most `max`.
pub fn pick_due(
    cards: &BTreeMap<String, Card>,
    candidates: &[String],
    day: f64,
    max: usize,
) -> Vec<String> {
    let mut due: Vec<(f64, &String)> = candidates
        .iter()
        .filter_map(|c| cards.get(c).map(|card| (card, c)))
        .filter(|(card, _)| card.due_day() <= day)
        .map(|(card, c)| (card.recall(day), c))
        .collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    due.into_iter().take(max).map(|(_, c)| c.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learn::fsrs::Grade;

    #[test]
    fn every_idea_has_a_question_that_its_own_answer_passes() {
        for c in CONCEPTS {
            for seed in 0..40 {
                let q = generate(c, seed).unwrap_or_else(|| panic!("{c}"));
                assert!(q.answer.is_finite() && !q.text.is_empty() && !q.working.is_empty());
                assert!(q.is_correct(q.answer), "{c} seed {seed}");
                assert!(!q.is_correct(q.answer * 1.5 + 1.0), "{c} seed {seed}");
                assert!(!q.is_correct(f64::NAN));
            }
        }
    }

    #[test]
    fn numbers_are_generated_from_the_seed() {
        for c in CONCEPTS {
            assert_eq!(generate(c, 7), generate(c, 7));
            let texts: std::collections::BTreeSet<_> = (0..40)
                .filter_map(|s| generate(c, s))
                .map(|q| q.text)
                .collect();
            assert!(
                texts.len() >= 3,
                "{c} only makes {} different questions",
                texts.len()
            );
        }
        assert!(generate("Nonsense", 1).is_none());
    }

    #[test]
    fn the_spec_example_is_computed_right() {
        // "A 56 mm wheel turns 3 times. How far does the robot go?"
        let found = (0..200)
            .filter_map(|s| generate("Wheel kinematics", s))
            .find(|q| q.text.starts_with("A 56 mm wheel turns 3 times"));
        let q = found.expect("the 56 mm / 3 turns question is one of the draws");
        assert!((q.answer - 527.78).abs() < 0.01, "{}", q.answer);
    }

    #[test]
    fn only_due_cards_of_earlier_ideas_are_picked_least_remembered_first() {
        let mut cards = BTreeMap::new();
        cards.insert("Actuators".to_owned(), Card::first(Grade::Good, 0.0)); // due on day 3.7
        cards.insert("Noise".to_owned(), Card::first(Grade::Again, 0.0)); // due on day 0.49
        cards.insert("Gyro".to_owned(), Card::first(Grade::Easy, 0.0)); // due on day 13.8
        let all: Vec<String> = ["Actuators", "Noise", "Gyro", "Odometry"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert!(pick_due(&cards, &all, 0.1, 2).is_empty());
        assert_eq!(pick_due(&cards, &all, 1.0, 2), vec!["Noise"]);
        assert_eq!(pick_due(&cards, &all, 5.0, 2), vec!["Noise", "Actuators"]);
        assert_eq!(pick_due(&cards, &all, 30.0, 3).len(), 3);
        assert_eq!(pick_due(&cards, &all[..1], 30.0, 3), vec!["Actuators"]);
    }
}
