use super::*;
use crate::def::{Criterion, Expected};
use crate::trace::{RunTrace, Sample};

/// A robot driving along +x at `v` m/s for `secs`, then stopped.
fn drive(v: f64, drive_s: f64, secs: f64) -> RunTrace {
    let mut t = RunTrace::default();
    let mut i = 0;
    while f64::from(i) * 0.01 <= secs {
        let time = f64::from(i) * 0.01;
        let moving = time < drive_s;
        let x = v * time.min(drive_s);
        t.samples.push(Sample {
            t: time,
            pos: [x, 0.0, 0.0],
            front: [x + 0.08, 0.0],
            speed: if moving { v } else { 0.0 },
            ahead_m: 1.0 - x,
            ..Sample::default()
        });
        i += 1;
    }
    t
}

fn ctx() -> EvalContext {
    EvalContext::default()
}

#[test]
fn final_x_says_how_far_off() {
    let c = Criterion::FinalX {
        x: 1.0,
        tol_m: 0.05,
        within_s: None,
    };
    let ok = check(&c, &ctx(), &drive(0.2, 5.0, 8.0));
    assert!(ok.ok, "{}", ok.message);
    let long = check(&c, &ctx(), &drive(0.25, 5.0, 8.0));
    assert!(!long.ok);
    assert!(long.message.contains("250 mm too far"), "{}", long.message);
    let short = check(&c, &ctx(), &drive(0.1, 5.0, 8.0));
    assert!(short.message.contains("short"), "{}", short.message);
}

#[test]
fn moving_robot_fails_a_stop() {
    let c = Criterion::FinalX {
        x: 1.0,
        tol_m: 0.05,
        within_s: None,
    };
    let r = check(
        &Criterion::FinalX {
            x: 0.5,
            tol_m: 5.0,
            within_s: None,
        },
        &ctx(),
        &drive(0.2, 6.0, 5.0),
    );
    assert!(!r.ok);
}

#[test]
fn front_stop_and_clearance() {
    let c = Criterion::FrontStop {
        min_x: 0.95,
        max_x: 1.05,
        within_s: 8.0,
    };
    assert!(check(&c, &ctx(), &drive(0.18, 5.0, 8.0)).ok);
    assert!(!check(&c, &ctx(), &drive(0.1, 5.0, 8.0)).ok);
    let c = Criterion::FrontClearance {
        min_m: 0.08,
        max_m: 0.12,
        within_s: 8.0,
    };
    assert!(check(&c, &ctx(), &drive(0.18, 5.0, 8.0)).ok);
}

#[test]
fn reports_compare_against_measured_values() {
    let mut t = drive(0.0, 0.0, 1.0);
    t.measured.insert("range".to_owned(), 2.0);
    t.reports.push((0.5, "range".to_owned(), 2.03));
    let c = Criterion::Report {
        key: "range".to_owned(),
        expected: Expected::Measured("range".to_owned()),
        tol: 0.05,
        within_s: None,
    };
    assert!(check(&c, &ctx(), &t).ok);
    t.reports.push((0.8, "range".to_owned(), 2.5));
    assert!(!check(&c, &ctx(), &t).ok);
}

#[test]
fn tipping_and_contact() {
    let mut t = drive(0.1, 1.0, 2.0);
    assert!(check(&Criterion::NoTip { max_tilt_rad: 0.3 }, &ctx(), &t).ok);
    t.samples[50].tilt = 0.5;
    assert!(!check(&Criterion::NoTip { max_tilt_rad: 0.3 }, &ctx(), &t).ok);
    t.samples[60].wall_contact = true;
    assert!(
        !check(
            &Criterion::NoContact {
                tag: "wall".to_owned()
            },
            &ctx(),
            &t
        )
        .ok
    );
}

#[test]
fn line_following_needs_the_lap_and_staying_close() {
    // A 1 m by 1 m square loop; a robot driving round it at 0.2 m/s.
    let path = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]];
    let ctx = EvalContext {
        zones: Vec::new(),
        line_path: Some(path),
    };
    let mut t = RunTrace::default();
    for i in 0..=2200 {
        let time = f64::from(i) * 0.01;
        let d = (0.2 * time).min(4.0);
        let (x, y) = if d < 1.0 {
            (d, 0.0)
        } else if d < 2.0 {
            (1.0, d - 1.0)
        } else if d < 3.0 {
            (3.0 - d, 1.0)
        } else {
            (0.0, 4.0 - d)
        };
        t.samples.push(Sample {
            t: time,
            pos: [x, y + 0.005, 0.0],
            speed: 0.2,
            ..Sample::default()
        });
    }
    let c = Criterion::FollowLine {
        max_dev_m: 0.03,
        laps: 1,
        within_s: 30.0,
    };
    assert!(check(&c, &ctx, &t).ok);
    let slow = Criterion::FollowLine {
        max_dev_m: 0.03,
        laps: 1,
        within_s: 10.0,
    };
    assert!(!check(&slow, &ctx, &t).ok);
    let tight = Criterion::FollowLine {
        max_dev_m: 0.002,
        laps: 1,
        within_s: 30.0,
    };
    assert!(!check(&tight, &ctx, &t).ok);
}

#[test]
fn an_error_in_the_program_fails_the_seed() {
    let mut t = drive(0.2, 5.0, 8.0);
    t.program_error = Some("trap".to_owned());
    let m = test_mission();
    let r = evaluate_seed(&m, &[], &ctx(), &t);
    assert!(!r.passed());
    assert_eq!(r.results[0].name, "program");
}

fn test_mission() -> crate::def::MissionDef {
    crate::def::MissionDef::from_ron(
        r#"(version: 1, id: "t", title: "T", chapter: 1, concepts: [],
        world: (id: "open_grid"), robot: Starter("rover"), time_limit_s: 10.0, grading_seeds: [1, 2],
        battery_soc: (1.0, 1.0), disabled_api: [], events: [], brief_md: "", predict: None,
        criteria: [FinalX(x: 1.0, tol_m: 0.05, within_s: None)],
        stars: ((text: "tight", criteria: [FinalX(x: 1.0, tol_m: 0.02, within_s: None)], no_hints: false),
                (text: "no hints", criteria: [], no_hints: true)),
        hints: ("a", "b", "c"))"#,
    )
    .expect("test mission parses")
}

#[test]
fn stars_are_earned_in_order_and_need_every_seed() {
    let m = test_mission();
    let exact = drive(0.2, 5.0, 8.0);
    let near = drive(0.2 * 1.03, 5.0, 8.0); // 30 mm long
    let g = grade(&m, &ctx(), &[exact.clone(), exact.clone()], false);
    assert_eq!((g.passed, g.stars), (true, 3));
    let g = grade(&m, &ctx(), &[exact.clone(), exact.clone()], true);
    assert_eq!(g.stars, 2);
    let g = grade(&m, &ctx(), &[exact, near], false);
    assert_eq!((g.passed, g.stars), (true, 1));
    let bad = drive(0.1, 5.0, 8.0);
    let g = grade(&m, &ctx(), &[bad], false);
    assert_eq!((g.passed, g.stars), (false, 0));
}
