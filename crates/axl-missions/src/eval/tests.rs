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

// ---- Every criterion, on synthetic paths -------------------------------------------------

/// A path through `(t, x, y, yaw)` keypoints, sampled every 10 ms with linear interpolation.
fn path(points: &[(f64, f64, f64, f64)]) -> RunTrace {
    let mut tr = RunTrace::default();
    let end = points.last().unwrap().0;
    let mut i = 0;
    while f64::from(i) * 0.01 <= end + 1e-9 {
        let t = f64::from(i) * 0.01;
        let k = points
            .windows(2)
            .position(|w| t <= w[1].0 + 1e-9)
            .unwrap_or(points.len() - 2);
        let (a, b) = (points[k], points[k + 1]);
        let f = if b.0 > a.0 {
            ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let x = a.1 + (b.1 - a.1) * f;
        let y = a.2 + (b.2 - a.2) * f;
        let speed = if f < 1.0 && b.0 > a.0 {
            ((b.1 - a.1).hypot(b.2 - a.2)) / (b.0 - a.0)
        } else {
            0.0
        };
        tr.samples.push(Sample {
            t,
            pos: [x, y, 0.0],
            yaw: a.3 + (b.3 - a.3) * f,
            speed,
            pivot: [x, y],
            ..Sample::default()
        });
        i += 1;
    }
    tr
}

fn zone_ctx() -> EvalContext {
    EvalContext {
        zones: vec![axl_world::Zone {
            name: "goal".into(),
            shape: axl_world::ZoneShape::Rect {
                min: [0.9, -0.1],
                max: [1.1, 0.1],
            },
        }],
        line_path: None,
    }
}

#[test]
fn reach_zone_and_finish_within_need_the_zone_in_time() {
    let tr = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (4.0, 1.0, 0.0, 0.0),
        (6.0, 1.0, 0.0, 0.0),
    ]);
    for make in [
        |within_s| Criterion::ReachZone {
            zone: "goal".into(),
            within_s,
        },
        |within_s| Criterion::FinishWithin {
            zone: "goal".into(),
            within_s,
        },
    ] {
        assert!(check(&make(5.0), &zone_ctx(), &tr).ok);
        let late = check(&make(3.0), &zone_ctx(), &tr);
        assert!(!late.ok);
        assert!(late.message.contains("too late"), "{}", late.message);
    }
    let never = path(&[(0.0, 0.0, 0.0, 0.0), (4.0, 0.3, 0.0, 0.0)]);
    let r = check(
        &Criterion::ReachZone {
            zone: "goal".into(),
            within_s: 9.0,
        },
        &zone_ctx(),
        &never,
    );
    assert!(!r.ok && r.message.contains("Never"), "{}", r.message);
    let unknown = check(
        &Criterion::ReachZone {
            zone: "nowhere".into(),
            within_s: 9.0,
        },
        &zone_ctx(),
        &tr,
    );
    assert!(!unknown.ok);
}

#[test]
fn final_pose_checks_position_and_heading() {
    let c = Criterion::FinalPose {
        x: 1.0,
        y: 0.5,
        yaw: std::f64::consts::FRAC_PI_2,
        pos_tol_m: 0.05,
        yaw_tol_rad: 0.1,
    };
    let good = path(&[(0.0, 0.0, 0.0, 0.0), (3.0, 1.02, 0.49, 1.6)]);
    assert!(check(&c, &ctx(), &good).ok);
    let off = path(&[(0.0, 0.0, 0.0, 0.0), (3.0, 1.2, 0.5, 1.57)]);
    assert!(!check(&c, &ctx(), &off).ok);
    let crooked = path(&[(0.0, 0.0, 0.0, 0.0), (3.0, 1.0, 0.5, 0.5)]);
    assert!(!check(&c, &ctx(), &crooked).ok);
    // The heading wraps: -3 pi / 2 is the same direction as pi / 2.
    let wrapped = path(&[(0.0, 0.0, 0.0, 0.0), (3.0, 1.0, 0.5, -4.712)]);
    assert!(check(&c, &ctx(), &wrapped).ok);
}

#[test]
fn turn_to_needs_the_angle_and_a_still_pivot() {
    let c = Criterion::TurnTo {
        yaw: std::f64::consts::PI,
        tol_rad: 0.05,
        drift_m: 0.03,
    };
    let good = path(&[(0.0, 0.0, 0.0, 0.0), (2.0, 0.01, 0.0, 3.15)]);
    assert!(check(&c, &ctx(), &good).ok);
    let under = path(&[(0.0, 0.0, 0.0, 0.0), (2.0, 0.0, 0.0, 2.9)]);
    assert!(!check(&c, &ctx(), &under).ok);
    // Right angle, but the robot swung wide.
    let wide = path(&[(0.0, 0.0, 0.0, 0.0), (2.0, 0.2, 0.0, 3.14)]);
    let r = check(&c, &ctx(), &wide);
    assert!(!r.ok && r.message.contains("drifted"), "{}", r.message);
    // Still turning at the end.
    let mut moving = good.clone();
    moving.samples.last_mut().unwrap().speed = 0.5;
    assert!(!check(&c, &ctx(), &moving).ok);
}

#[test]
fn waypoints_must_be_visited_in_order_and_the_robot_must_come_home() {
    let c = Criterion::Waypoints {
        points: vec![[0.5, 0.0], [0.5, 0.5], [0.0, 0.5]],
        tol_m: 0.05,
        end_tol_m: 0.1,
    };
    let square = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (1.0, 0.5, 0.0, 0.0),
        (2.0, 0.5, 0.5, 0.0),
        (3.0, 0.0, 0.5, 0.0),
        (4.0, 0.0, 0.0, 0.0),
    ]);
    assert!(check(&c, &ctx(), &square).ok);
    let wrong_way = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (1.0, 0.0, 0.5, 0.0),
        (2.0, 0.5, 0.5, 0.0),
        (3.0, 0.5, 0.0, 0.0),
        (4.0, 0.0, 0.0, 0.0),
    ]);
    assert!(!check(&c, &ctx(), &wrong_way).ok);
    let no_return = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (1.0, 0.5, 0.0, 0.0),
        (2.0, 0.5, 0.5, 0.0),
        (3.0, 0.0, 0.5, 0.0),
    ]);
    let r = check(&c, &ctx(), &no_return);
    assert!(
        !r.ok && r.message.contains("from the start"),
        "{}",
        r.message
    );
    let missed = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (1.0, 0.5, 0.0, 0.0),
        (2.0, 0.0, 0.0, 0.0),
    ]);
    let r = check(&c, &ctx(), &missed);
    assert!(!r.ok && r.message.contains("corner 2"), "{}", r.message);
}

#[test]
fn mean_speed_is_taken_over_its_window_only() {
    let c = Criterion::MeanSpeed {
        from_s: 2.0,
        to_s: 4.0,
        target_ms: 0.2,
        tol_ms: 0.02,
    };
    // Slow start (0.05 m/s for 2 s), then 0.2 m/s.
    let tr = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (2.0, 0.1, 0.0, 0.0),
        (6.0, 0.9, 0.0, 0.0),
    ]);
    assert!(check(&c, &ctx(), &tr).ok);
    let slow = path(&[(0.0, 0.0, 0.0, 0.0), (6.0, 0.6, 0.0, 0.0)]);
    assert!(!check(&c, &ctx(), &slow).ok);
    let short = path(&[(0.0, 0.0, 0.0, 0.0), (1.0, 0.2, 0.0, 0.0)]);
    let r = check(&c, &ctx(), &short);
    assert!(!r.ok && r.message.contains("shorter"), "{}", r.message);
}

#[test]
fn heading_lock_checks_distance_sideways_drift_and_yaw() {
    let c = Criterion::HeadingLock {
        distance_m: 2.0,
        max_lateral_m: 0.05,
        max_yaw_rad: 0.05,
    };
    let good = path(&[(0.0, 0.0, 0.0, 0.0), (8.0, 2.01, 0.02, 0.02)]);
    assert!(check(&c, &ctx(), &good).ok);
    let drifted = path(&[(0.0, 0.0, 0.0, 0.0), (8.0, 2.0, 0.2, 0.02)]);
    assert!(!check(&c, &ctx(), &drifted).ok);
    let crooked = path(&[(0.0, 0.0, 0.0, 0.0), (8.0, 2.0, 0.0, 0.2)]);
    assert!(!check(&c, &ctx(), &crooked).ok);
    let short = path(&[(0.0, 0.0, 0.0, 0.0), (8.0, 1.5, 0.0, 0.0)]);
    assert!(!check(&c, &ctx(), &short).ok);
}

#[test]
fn believed_pose_needs_a_mark_close_to_the_truth() {
    let c = Criterion::BelievedPose { tol_m: 0.05 };
    let mut tr = path(&[(0.0, 0.0, 0.0, 0.0), (4.0, 1.0, 0.5, 0.0)]);
    let r = check(&c, &ctx(), &tr);
    assert!(!r.ok && r.message.contains("never called"), "{}", r.message);
    tr.believed.push((3.9, [1.02, 0.51, 0.0]));
    assert!(check(&c, &ctx(), &tr).ok);
    tr.believed.push((4.0, [1.3, 0.5, 0.0]));
    assert!(!check(&c, &ctx(), &tr).ok, "the last mark counts");
}

#[test]
fn end_near_start_plot_and_parts() {
    let c = Criterion::EndNearStart { tol_m: 0.1 };
    let home = path(&[
        (0.0, 0.0, 0.0, 0.0),
        (2.0, 1.0, 0.0, 0.0),
        (4.0, 0.03, 0.0, 0.0),
    ]);
    assert!(check(&c, &ctx(), &home).ok);
    let away = path(&[(0.0, 0.0, 0.0, 0.0), (2.0, 1.0, 0.0, 0.0)]);
    assert!(!check(&c, &ctx(), &away).ok);

    let mut tr = path(&[(0.0, 0.0, 0.0, 0.0), (1.0, 0.0, 0.0, 0.0)]);
    assert!(!check(&Criterion::UsedPlot, &ctx(), &tr).ok);
    tr.plot_used = true;
    assert!(check(&Criterion::UsedPlot, &ctx(), &tr).ok);

    tr.part_count = 12;
    assert!(check(&Criterion::MaxParts { n: 12 }, &ctx(), &tr).ok);
    assert!(!check(&Criterion::MaxParts { n: 11 }, &ctx(), &tr).ok);
}
