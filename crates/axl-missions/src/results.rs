//! What the Results screen shows, as plain data: one row per criterion, the numbers of each seed,
//! the real answer to the prediction question, and the brief as headings and paragraphs. The screen
//! itself (egui) only draws these.

use crate::def::{Criterion, Expected, MissionDef, MissionEvent, RobotSource};
use crate::eval::Grade;
use crate::trace::RunTrace;

/// One criterion over all seeds.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Short name.
    pub name: String,
    /// It held on every seed.
    pub ok: bool,
    /// What happened: the first failing seed's message, else the first seed's.
    pub message: String,
}

/// The mission's criteria as rows. A criterion passes when it holds on every seed; a failing row
/// says which seed failed.
pub fn rows(g: &Grade) -> Vec<Row> {
    let Some(first) = g.seeds.first() else {
        return Vec::new();
    };
    (0..first.results.len())
        .map(|i| {
            let failing = g
                .seeds
                .iter()
                .find(|s| s.results.get(i).is_some_and(|r| !r.ok));
            match failing {
                Some(s) => Row {
                    name: s.results[i].name.clone(),
                    ok: false,
                    message: if g.seeds.len() > 1 {
                        format!("Seed {}: {}", s.seed, s.results[i].message)
                    } else {
                        s.results[i].message.clone()
                    },
                },
                None => Row {
                    name: first.results[i].name.clone(),
                    ok: true,
                    message: first.results[i].message.clone(),
                },
            }
        })
        .collect()
}

/// Numbers for one seed.
#[derive(Clone, Debug, PartialEq)]
pub struct SeedMetrics {
    /// The seed.
    pub seed: u64,
    /// Every criterion held.
    pub passed: bool,
    /// Simulated seconds of the run.
    pub time_s: f64,
    /// Energy used, watt-hours.
    pub energy_wh: f64,
    /// Contacts with walls and obstacles.
    pub collisions: u32,
}

/// The metrics of each seed (`traces` are in the same order as `g.seeds`).
pub fn metrics(g: &Grade, traces: &[RunTrace]) -> Vec<SeedMetrics> {
    g.seeds
        .iter()
        .zip(traces)
        .map(|(s, t)| SeedMetrics {
            seed: s.seed,
            passed: s.passed(),
            time_s: t.last().map_or(0.0, |l| l.t),
            energy_wh: t.energy_wh,
            collisions: t.collisions,
        })
        .collect()
}

/// The real answer to a prediction question: a measured number of the world, or where the robot
/// ended (`final_x`, `final_y`).
pub fn answer_of(t: &RunTrace, key: &str) -> Option<f64> {
    match key {
        "final_x" => t.last().map(|s| s.pos[0]),
        "final_y" => t.last().map(|s| s.pos[1]),
        _ => t.measured.get(key).copied(),
    }
}

/// How close a prediction was, in words that teach rather than score.
pub fn prediction_verdict(predicted: f64, actual: f64, unit: &str) -> String {
    let off = (predicted - actual).abs();
    let rel = if actual.abs() > 1e-9 {
        off / actual.abs()
    } else {
        off
    };
    let how = if rel <= 0.05 {
        "Very close."
    } else if rel <= 0.25 {
        "Not far off."
    } else {
        "A surprise, and surprises teach the most."
    };
    format!("You predicted {predicted:.3} {unit}; the simulation shows {actual:.3} {unit}. {how}")
}

fn mm(m: f64) -> String {
    format!("{:.0} mm", m * 1000.0)
}

fn deg(r: f64) -> String {
    format!("{:.0} degrees", r.to_degrees())
}

/// The headline of the brief: its first Markdown heading, or the title.
pub fn goal_line(m: &MissionDef) -> String {
    blocks(&m.brief_md)
        .into_iter()
        .find_map(|b| match b {
            Block::Heading(h) => Some(h),
            _ => None,
        })
        .unwrap_or_else(|| m.title.clone())
}

/// The rules of a mission in plain sentences: time, seeds, battery, what is switched off, pushes.
pub fn rules(m: &MissionDef) -> Vec<String> {
    let mut v = vec![format!("The run ends after {} s.", m.time_limit_s)];
    let n = m.grading_seeds.len();
    v.push(if n == 1 {
        "Your program is graded once.".to_owned()
    } else {
        format!(
            "Your program is graded on {n} runs (seeds {}); it has to succeed on every one.",
            m.grading_seeds
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    v.push(format!(
        "The battery starts {}% charged{}.",
        (m.battery_soc.1 * 100.0).round(),
        if (m.battery_soc.1 - m.battery_soc.0).abs() > 1e-9 {
            format!(
                ", or anywhere from {}% to {}%, differently on each run",
                (m.battery_soc.0 * 100.0).round(),
                (m.battery_soc.1 * 100.0).round()
            )
        } else {
            String::new()
        }
    ));
    for api in &m.disabled_api {
        v.push(format!(
            "This mission switches off {api} so you can build your own controller."
        ));
    }
    for e in &m.events {
        match e {
            MissionEvent::Push { t, impulse_ns } => v.push(format!(
                "The Rover is pushed at {t} s (an impulse of {:.2} N s).",
                impulse_ns.iter().map(|x| x * x).sum::<f64>().sqrt()
            )),
        }
    }
    v.push("Three hints unlock after your first run that does not pass.".to_owned());
    v
}

/// The starter robot (or the building rules) in words.
pub fn robot_text(m: &MissionDef) -> String {
    match &m.robot {
        RobotSource::Starter(id) => match id.as_str() {
            "rover" => "The Rover: two medium motors (left, right) with 56 mm wheels, a ball caster, an ultrasonic sensor (eye) and a colour sensor (color).".to_owned(),
            "rover_tof" => "The Rover with two distance sensors, the ultrasonic eye and the time-of-flight sensor tof, and the colour sensor color.".to_owned(),
            other => format!("The starter robot \"{other}\"."),
        },
        RobotSource::Build { max_parts, allowed } => format!(
            "You build the robot in the Builder, with at most {max_parts} parts{}.",
            if allowed.is_empty() {
                String::new()
            } else {
                format!(", from {} allowed parts", allowed.len())
            }
        ),
    }
}

/// A criterion in words a learner can read before the run.
pub fn describe(c: &Criterion) -> String {
    match c {
        Criterion::ReachZone { zone, within_s } | Criterion::FinishWithin { zone, within_s } => {
            format!("Get the robot into the zone \"{zone}\" within {within_s} s.")
        }
        Criterion::FinalPose {
            x,
            y,
            yaw,
            pos_tol_m,
            yaw_tol_rad,
        } => format!(
            "End at x = {x} m, y = {y} m, facing {} (within {} and {}).",
            deg(*yaw),
            mm(*pos_tol_m),
            deg(*yaw_tol_rad)
        ),
        Criterion::FinalX { x, tol_m, within_s } => format!(
            "Stop with the hub at x = {x:.3} m, within {}{}.",
            mm(*tol_m),
            within_s.map_or(String::new(), |s| format!(", in {s} s"))
        ),
        Criterion::FrontStop {
            min_x,
            max_x,
            within_s,
        } => format!(
            "Stop with the front of the robot between x = {min_x} m and {max_x} m within {within_s} s."
        ),
        Criterion::TurnTo {
            yaw,
            tol_rad,
            drift_m,
        } => format!(
            "Turn {} in total (within {}), without drifting more than {}.",
            deg(*yaw),
            deg(*tol_rad),
            mm(*drift_m)
        ),
        Criterion::Waypoints {
            points,
            tol_m,
            end_tol_m,
        } => format!(
            "Pass within {} of {} corners in order, then end within {} of the start.",
            mm(*tol_m),
            points.len(),
            mm(*end_tol_m)
        ),
        Criterion::NoContact { tag } => format!("Never touch the {tag}."),
        Criterion::FrontClearance {
            min_m,
            max_m,
            within_s,
        } => format!(
            "Stop {} to {} from what is ahead, within {within_s} s.",
            mm(*min_m),
            mm(*max_m)
        ),
        Criterion::FollowLine {
            max_dev_m,
            laps,
            within_s,
        } => format!(
            "Follow the line for {laps} lap(s) in {within_s} s, never more than {} from it.",
            mm(*max_dev_m)
        ),
        Criterion::MeanSpeed {
            from_s,
            to_s,
            target_ms,
            tol_ms,
        } => format!(
            "Drive at {target_ms} m/s (+- {tol_ms}) on average from {from_s} s to {to_s} s."
        ),
        Criterion::Report {
            key, expected, tol, ..
        } => match expected {
            Expected::Value(v) => format!("Report \"{key}\" = {v} (+- {tol}) with axl_report."),
            Expected::Measured(_) => format!(
                "Report \"{key}\" with axl_report; the simulator checks it against its own measurement (+- {tol})."
            ),
        },
        Criterion::NoTip { max_tilt_rad } => {
            format!("Never tilt more than {}.", deg(*max_tilt_rad))
        }
        Criterion::HeadingLock {
            distance_m,
            max_lateral_m,
            max_yaw_rad,
        } => format!(
            "Drive straight for {distance_m} m: at most {} to the side and {} off in heading.",
            mm(*max_lateral_m),
            deg(*max_yaw_rad)
        ),
        Criterion::BelievedPose { tol_m } => format!(
            "Keep track of your position with axl_mark_pose; it must end within {} of the truth.",
            mm(*tol_m)
        ),
        Criterion::EndNearStart { tol_m } => format!("End within {} of the start.", mm(*tol_m)),
        Criterion::UsedPlot => "Send something to the plotter with axl_plot.".to_owned(),
        Criterion::MaxParts { n } => format!("Use at most {n} parts."),
    }
}

/// A piece of a brief.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A heading.
    Heading(String),
    /// A paragraph, lines joined.
    Para(String),
    /// A list item.
    Bullet(String),
}

fn strip_inline(s: &str) -> String {
    s.replace("**", "").replace(['`', '*'], "")
}

/// Turn the brief's Markdown into blocks: `#` headings, `-` bullets, paragraphs split at blank
/// lines. Bold and code marks are dropped.
pub fn blocks(md: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(para.join(" ")));
            para.clear();
        }
    };
    for line in md.lines() {
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
        } else if t.starts_with('#') {
            flush(&mut para, &mut out);
            out.push(Block::Heading(strip_inline(
                t.trim_start_matches('#').trim(),
            )));
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut out);
            out.push(Block::Bullet(strip_inline(item)));
        } else {
            para.push(strip_inline(t));
        }
    }
    flush(&mut para, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{CriterionResult, SeedResult};
    use crate::trace::Sample;

    fn result(name: &str, ok: bool) -> CriterionResult {
        CriterionResult {
            name: name.into(),
            ok,
            message: format!("{name} {ok}"),
        }
    }

    #[test]
    fn every_mission_has_a_goal_rules_and_a_robot_description() {
        for m in crate::library::builtin_missions().unwrap() {
            assert!(!goal_line(&m).is_empty(), "{}", m.id);
            let rules = rules(&m);
            assert!(
                rules[0].contains(&format!("{}", m.time_limit_s)),
                "{}",
                m.id
            );
            assert!(rules.iter().any(|r| r.contains("battery")), "{}", m.id);
            for api in &m.disabled_api {
                assert!(rules.iter().any(|r| r.contains(api.as_str())), "{}", m.id);
            }
            assert_eq!(
                rules.iter().filter(|r| r.contains("is pushed")).count(),
                m.events.len()
            );
            assert!(!robot_text(&m).is_empty());
        }
        let m5 = crate::library::mission("m05").unwrap();
        assert_eq!(goal_line(&m5), "Stop 100 mm from the wall");
        assert!(robot_text(&m5).contains("time-of-flight"));
        let m12 = crate::library::mission("m12").unwrap();
        assert!(robot_text(&m12).contains("at most 120 parts"));
        let m7 = crate::library::mission("m07").unwrap();
        assert!(rules(&m7).iter().any(|r| r.contains("motor_speed")));
    }

    fn grade(seeds: Vec<SeedResult>) -> Grade {
        Grade {
            passed: seeds.iter().all(SeedResult::passed),
            stars: 0,
            seeds,
            star_seeds: [Vec::new(), Vec::new()],
        }
    }

    #[test]
    fn a_criterion_must_hold_on_every_seed_and_names_the_failing_seed() {
        let g = grade(vec![
            SeedResult {
                seed: 1,
                results: vec![result("a", true), result("b", true)],
            },
            SeedResult {
                seed: 2,
                results: vec![result("a", true), result("b", false)],
            },
        ]);
        let r = rows(&g);
        assert_eq!(r.len(), 2);
        assert!(r[0].ok);
        assert!(!r[1].ok);
        assert!(r[1].message.starts_with("Seed 2: "), "{}", r[1].message);
        assert!(rows(&grade(vec![])).is_empty());
    }

    #[test]
    fn metrics_and_answers_come_from_the_trace() {
        let mut t = RunTrace {
            energy_wh: 0.5,
            collisions: 2,
            ..RunTrace::default()
        };
        t.samples.push(Sample {
            t: 3.5,
            pos: [1.25, -0.5, 0.0],
            ..Sample::default()
        });
        t.measured.insert("k".into(), 7.0);
        let g = grade(vec![SeedResult {
            seed: 4,
            results: vec![result("a", true)],
        }]);
        let m = metrics(&g, std::slice::from_ref(&t));
        assert_eq!(m[0].seed, 4);
        assert!(m[0].passed);
        assert_eq!(
            (m[0].time_s, m[0].energy_wh, m[0].collisions),
            (3.5, 0.5, 2)
        );
        assert_eq!(answer_of(&t, "final_x"), Some(1.25));
        assert_eq!(answer_of(&t, "final_y"), Some(-0.5));
        assert_eq!(answer_of(&t, "k"), Some(7.0));
        assert_eq!(answer_of(&t, "nope"), None);
    }

    #[test]
    fn the_verdict_compares_prediction_and_truth() {
        assert!(prediction_verdict(100.0, 102.0, "mm").contains("Very close"));
        assert!(prediction_verdict(100.0, 120.0, "mm").contains("Not far off"));
        assert!(prediction_verdict(10.0, 120.0, "mm").contains("surprise"));
        assert!(prediction_verdict(1.0, 2.0, "mm").contains("1.000 mm"));
    }

    #[test]
    fn every_criterion_of_every_mission_can_be_described() {
        for m in crate::library::builtin_missions().unwrap() {
            for c in m
                .criteria
                .iter()
                .chain(m.stars.iter().flat_map(|s| &s.criteria))
            {
                assert!(describe(c).len() > 10, "{} {c:?}", m.id);
            }
        }
    }

    #[test]
    fn the_brief_becomes_blocks() {
        let b = blocks("## Title\n\nOne **bold** line\nand more.\n\n- a\n- `b`\n");
        assert_eq!(
            b,
            vec![
                Block::Heading("Title".into()),
                Block::Para("One bold line and more.".into()),
                Block::Bullet("a".into()),
                Block::Bullet("b".into()),
            ]
        );
    }
}
