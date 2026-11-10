//! The number measured for each mission's prediction question must be the quantity the question
//! asks about. One test per mission, each against an expectation worked out another way (from
//! the spec's formulas, the world files or the robot's geometry), plus a check that every
//! mission's prediction key has a measurement at all.

use std::path::Path;

use axl_build::model::Assembly;
use axl_missions::library::{builtin_missions, mission};
use axl_parts::catalog::Catalog;
use axl_sim::mission::{measure, robot_assembly};
use axl_world::builtin_worlds;

const WHEEL_RADIUS_M: f64 = 0.028; // wheel_56: 56 mm diameter
const G: f64 = 9.80665;

fn design(id: &str) -> Option<std::path::PathBuf> {
    (id == "m12")
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../missions/solutions/m12.axlbot"))
}

fn robot(id: &str, cat: &Catalog) -> Assembly {
    robot_assembly(&mission(id).unwrap(), cat, design(id).as_deref()).unwrap()
}

/// The measured answer of mission `id`'s prediction question.
fn measured(id: &str) -> f64 {
    let cat = Catalog::builtin();
    let m = mission(id).unwrap();
    let key = m.predict.as_ref().expect("a prediction").measured.clone();
    measure(&key, &m, &robot(id, &cat), &cat)
        .unwrap()
        .unwrap_or_else(|| panic!("{id}: no measurement for {key}"))
}

fn near(what: &str, got: f64, want: f64, rel: f64) {
    assert!(
        (got - want).abs() <= want.abs() * rel,
        "{what}: measured {got}, expected {want} within {:.1}%",
        rel * 100.0
    );
}

/// Distance between the two wheel centres of the Rover, from the design file.
fn wheel_spacing_m(asm: &Assembly) -> f64 {
    let ys: Vec<f64> = asm
        .parts
        .iter()
        .filter(|p| p.part.0 == "wheel_56")
        .map(|p| f64::from(p.pos[1]) * 1e-4)
        .collect();
    assert_eq!(ys.len(), 2);
    (ys[0] - ys[1]).abs()
}

#[test]
fn every_prediction_key_has_a_measurement() {
    for m in builtin_missions().unwrap() {
        let p = m
            .predict
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no prediction", m.id));
        if p.measured == "final_x" || p.measured == "final_y" {
            continue;
        }
        let cat = Catalog::builtin();
        let v = measure(&p.measured, &m, &robot(&m.id, &cat), &cat).unwrap();
        assert!(
            v.is_some_and(f64::is_finite),
            "{}: {} not measured",
            m.id,
            p.measured
        );
    }
}

#[test]
fn m01_distance_at_half_power_for_two_seconds() {
    // Half power at the battery voltage of the run, no-load speed from the spec's motor table
    // (26.2 rad/s at 7.4 V): the Rover cannot beat the steady speed, and the start-up takes
    // under a quarter of the two seconds.
    let m = mission("m01").unwrap();
    let volts = 6.4 + (8.4 - 6.4) * m.soc_for(1);
    let ideal_mm = 26.2 * (0.5 * volts / 7.4) * WHEEL_RADIUS_M * 2.0 * 1000.0;
    let got = measured("m01");
    assert!(
        got < ideal_mm && got > 0.75 * ideal_mm,
        "{got} mm, ideal {ideal_mm} mm"
    );
}

#[test]
fn m02_distance_for_one_wheel_turn() {
    // One turn of a 56 mm wheel rolls its circumference (a little less with slip).
    near(
        "m02",
        measured("m02"),
        std::f64::consts::TAU * WHEEL_RADIUS_M * 1000.0,
        0.05,
    );
}

#[test]
fn m03_degrees_turned_when_the_wheels_turn_once_in_opposite_directions() {
    // Each wheel rolls one circumference, in opposite directions: turn = 2 * 2 pi r / spacing.
    let cat = Catalog::builtin();
    let b = wheel_spacing_m(&robot("m03", &cat));
    let want = (2.0 * std::f64::consts::TAU * WHEEL_RADIUS_M / b).to_degrees();
    // 140 degrees, not the 360 that a learner may guess: that is the point of the question.
    assert!((130.0..150.0).contains(&want));
    near("m03", measured("m03"), want, 0.03);
}

#[test]
fn m04_distance_from_the_start_after_a_square_with_turns_two_degrees_short() {
    // Four sides of 0.5 m with headings 0, 88, 176 and 264 degrees: the vectors sum to
    // 0.5 * sin(4 * 88 / 2) / sin(88 / 2) metres.
    let want_mm = 0.5 * (176.0_f64.to_radians().sin() / 44.0_f64.to_radians().sin()) * 1000.0;
    near("m04", measured("m04"), want_mm, 0.001);
}

#[test]
fn m05_asks_about_the_ultrasonic_sensor_and_it_does_not_see_the_angled_wall() {
    // The answer is 0 (no). The robot also carries a time-of-flight sensor that does see the
    // wall, so the measurement must come from the ultrasonic one, whatever the device order.
    assert_eq!(measured("m05"), 0.0);
    let m = mission("m05").unwrap();
    assert_eq!(m.world.variant_for(3), Some("angled"));
}

#[test]
fn m06_spread_of_forty_distance_readings() {
    // Spec: sigma = 3 mm + 1% of the distance, then rounded to 1 mm (adds 1/12 mm^2 of variance).
    let cat = Catalog::builtin();
    let m = mission("m06").unwrap();
    let range_m = measure("sensor_range_m", &m, &robot("m06", &cat), &cat)
        .unwrap()
        .unwrap();
    let sigma = 3.0 + 10.0 * range_m;
    let want = (sigma * sigma + 1.0 / 12.0).sqrt();
    near("m06", measured("m06"), want, 0.30);
}

#[test]
fn m07_speed_up_the_ramp_between_two_and_five_seconds() {
    // Never faster than the no-load speed at half the battery voltage, and clearly moving.
    let m = mission("m07").unwrap();
    let volts = 6.4 + (8.4 - 6.4) * m.soc_for(1);
    let free_ms = 26.2 * (0.5 * volts / 7.4) * WHEEL_RADIUS_M;
    let got = measured("m07");
    assert!(
        got > 0.3 * free_ms && got < free_ms,
        "{got} m/s, free speed {free_ms} m/s"
    );
}

#[test]
fn m08_reflect_value_over_the_black_line() {
    // The sensor must be over the printed line at the start, and the line is black.
    let got = measured("m08");
    assert!((0.0..=10.0).contains(&got), "{got}");
}

#[test]
fn m09_lap_time_at_a_steady_speed_is_the_length_of_the_printed_line_over_0_15() {
    let worlds = builtin_worlds().unwrap();
    let w = &worlds["line_track"];
    let v = w.variants.iter().find(|v| v.id == "b").unwrap();
    let path = v.path.as_ref().unwrap();
    assert_eq!(path.first(), path.last(), "the guide line is a closed loop");
    let len = |pts: &[[f64; 2]]| -> f64 {
        pts.windows(2)
            .map(|p| (p[0][0] - p[1][0]).hypot(p[0][1] - p[1][1]))
            .sum()
    };
    // The guide line follows what is printed on the mat.
    let printed = v
        .mat_shapes
        .as_ref()
        .and_then(|s| {
            s.iter().find_map(|s| match s {
                axl_world::MatShape::Line { points, .. } => Some(points.clone()),
                _ => None,
            })
        })
        .expect("a printed line");
    near("printed vs guide length", len(&printed), len(path), 0.02);
    near("m09", measured("m09"), len(path) / 0.15, 1e-6);
}

#[test]
fn m10_sideways_drift_in_three_seconds_with_the_two_pushes() {
    // The pushes at 0.5 s and 1.0 s are what make the Rover drift; without them it goes straight.
    let cat = Catalog::builtin();
    let mut m = mission("m10").unwrap();
    assert_eq!(m.events.len(), 2);
    let with = measured("m10");
    assert!(with > 40.0, "pushes give {with} mm");
    m.events.clear();
    let asm = robot("m10", &cat);
    let without = measure("open_loop_drift_mm", &m, &asm, &cat)
        .unwrap()
        .unwrap();
    assert!(without < 15.0, "no pushes gives {without} mm");
}

#[test]
fn m11_moves_on_the_shortest_way_through_the_first_layout() {
    // Breadth-first search over the cell walls, written out here again.
    let worlds = builtin_worlds().unwrap();
    let mz = worlds["maze"].maze.as_ref().unwrap();
    let g = axl_world::maze::generate(mz.cells, mz.layout_seeds[0]);
    let n = mz.cells;
    let mut dist = vec![None; n * n];
    dist[0] = Some(0usize);
    let mut todo = std::collections::VecDeque::from([(0usize, 0usize)]);
    while let Some((i, j)) = todo.pop_front() {
        for (di, dj) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let (ni, nj) = (i as i32 + di, j as i32 + dj);
            if ni < 0 || nj < 0 || ni >= n as i32 || nj >= n as i32 {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            if g.open(i, j, di, dj) && dist[nj * n + ni].is_none() {
                dist[nj * n + ni] = dist[j * n + i].map(|d| d + 1);
                todo.push_back((ni, nj));
            }
        }
    }
    let want = dist[n * n - 1].unwrap();
    // The grid is 6 x 6: at least 10 moves, and the parity of 10.
    assert!(want >= 10 && want % 2 == 0);
    assert_eq!(measured("m11"), want as f64);
    // The question says moves, so the unit must too.
    let p = mission("m11").unwrap().predict.unwrap();
    assert_eq!(p.unit, "moves");
}

#[test]
fn m12_force_to_hold_one_kilogram_on_the_ramp() {
    // The angle comes from the ramp that the mission's world variant puts in the world.
    let worlds = builtin_worlds().unwrap();
    let m = mission("m12").unwrap();
    let variant = m.world.variant.as_deref().unwrap();
    let ramp = worlds["ramp_lab"]
        .statics
        .iter()
        .find(|s| s.name == variant)
        .expect("a ramp named like the variant");
    let angle = ramp.euler_deg[1].abs().to_radians();
    assert!(m.predict.unwrap().question.contains("25 degree"));
    near("m12", measured("m12"), G * angle.sin(), 1e-9);
}
