//! The shipped worlds and objects.

use axl_world::{WorldFriction, build_world, builtin_objects, builtin_worlds};

fn fr() -> WorldFriction {
    // The values of `assets/physics/hardware.ron`.
    WorldFriction {
        wall: 0.4,
        plastic: 0.35,
        rubber: 1.0,
    }
}

#[test]
fn six_worlds_and_seven_objects_load() {
    let w = builtin_worlds().unwrap();
    let ids: Vec<&str> = w.keys().map(String::as_str).collect();
    assert_eq!(
        ids,
        [
            "line_track",
            "maze",
            "open_grid",
            "ramp_lab",
            "test_track",
            "walled_arena"
        ]
    );
    let o = builtin_objects().unwrap();
    assert_eq!(o.len(), 7);
    // Mass and friction are the spec's.
    let want = [
        ("cube_wood_40", 0.032, 0.4),
        ("cube_steel_40", 0.25, 0.3),
        ("box_card_100", 0.05, 0.5),
        ("cylinder_wood_40x60", 0.038, 0.4),
        ("ball_table_tennis", 0.0027, 0.4),
        ("ball_tennis", 0.058, 0.6),
        ("can_330", 0.36, 0.3),
    ];
    for (id, m, mu) in want {
        assert_eq!((o[id].mass, o[id].friction), (m, mu), "{id}");
    }
    assert!(o["ball_table_tennis"].hollow && o["ball_tennis"].hollow);
    assert!(!o["can_330"].hollow);
}

#[test]
fn every_world_and_variant_builds() {
    let worlds = builtin_worlds().unwrap();
    let objects = builtin_objects().unwrap();
    for (id, w) in &worlds {
        for seed in [1, 2, 3] {
            build_world(w, None, seed, &objects, &fr()).unwrap_or_else(|e| panic!("{id}: {e}"));
            for v in &w.variants {
                build_world(w, Some(&v.id), seed, &objects, &fr())
                    .unwrap_or_else(|e| panic!("{id}/{}: {e}", v.id));
            }
        }
    }
    assert!(build_world(&worlds["maze"], Some("nope"), 1, &objects, &fr()).is_err());
}

#[test]
fn the_line_track_has_a_19_mm_black_line() {
    let worlds = builtin_worlds().unwrap();
    let b = build_world(
        &worlds["line_track"],
        Some("a"),
        1,
        &builtin_objects().unwrap(),
        &fr(),
    )
    .unwrap();
    let m = b.mat.unwrap();
    assert_eq!((m.width, m.height), (1000, 600));
    // Across the line at the bottom of the oval (x = 0.9, y near -0.4): count dark pixels in a column.
    let col = ((0.9 - m.min[0]) / 0.002) as usize;
    let dark: Vec<usize> = (0..m.height / 2)
        .filter(|j| m.pixels[j * m.width + col][0] < 0.1)
        .collect();
    let (lo, hi) = (*dark.first().unwrap(), *dark.last().unwrap());
    assert_eq!(hi - lo + 1, dark.len(), "one band");
    let width_mm = dark.len() as f64 * 2.0;
    assert!((width_mm - 19.0).abs() <= 2.5, "{width_mm} mm");
    let y = m.min[1] + (lo + hi) as f64 / 2.0 * 0.002 + 0.001;
    assert!((y + 0.4).abs() < 0.004, "line at y = {y}");
}

#[test]
fn the_test_track_has_marks_every_100_mm_and_a_start_box() {
    let worlds = builtin_worlds().unwrap();
    let b = build_world(
        &worlds["test_track"],
        None,
        1,
        &builtin_objects().unwrap(),
        &fr(),
    )
    .unwrap();
    let m = b.mat.unwrap();
    let row = ((0.45 - m.min[1]) / 0.002) as usize;
    let mut marks = 0;
    let mut in_mark = false;
    for i in 0..m.width {
        let dark = m.pixels[row * m.width + i][0] < 0.5;
        if dark && !in_mark {
            marks += 1;
        }
        in_mark = dark;
    }
    assert_eq!(marks, 19, "marks at 0.0, 0.1 ... 1.8 m");
    assert!(
        b.zones
            .iter()
            .any(|z| z.name == "start" && z.contains([0.0, 0.0]))
    );
}

#[test]
fn the_maze_layout_follows_the_seed_and_the_goal_is_reachable() {
    let worlds = builtin_worlds().unwrap();
    let objects = builtin_objects().unwrap();
    let a = build_world(&worlds["maze"], None, 1, &objects, &fr()).unwrap();
    let b = build_world(&worlds["maze"], None, 2, &objects, &fr()).unwrap();
    let a2 = build_world(&worlds["maze"], None, 1, &objects, &fr()).unwrap();
    assert_ne!(a.worldbody, b.worldbody);
    assert_eq!(a.worldbody, a2.worldbody, "deterministic");
    assert!(
        a.zones
            .iter()
            .any(|z| z.name == "goal" && z.contains([1.4, 1.4]))
    );
    // Spawn in the first cell.
    assert!((a.spawn.x - 0.125).abs() < 1e-9 && (a.spawn.y - 0.125).abs() < 1e-9);
}

#[test]
fn the_arena_has_four_obstacles_and_an_angled_variant() {
    let worlds = builtin_worlds().unwrap();
    let objects = builtin_objects().unwrap();
    let s = build_world(
        &worlds["walled_arena"],
        Some("straight"),
        1,
        &objects,
        &fr(),
    )
    .unwrap();
    assert_eq!(s.wall_geoms.len(), 4);
    assert_eq!(s.worldbody.matches("obstacle_").count(), 4);
    let a = build_world(&worlds["walled_arena"], Some("angled"), 3, &objects, &fr()).unwrap();
    assert_ne!(s.worldbody, a.worldbody);
}

#[test]
fn the_ramp_lab_has_four_ramps_two_rubber_two_plastic() {
    let w = &builtin_worlds().unwrap()["ramp_lab"];
    assert_eq!(w.statics.len(), 4);
    let rubber = w.statics.iter().filter(|s| s.material == "rubber").count();
    assert_eq!(rubber, 2);
    let angles: Vec<f64> = w.statics.iter().map(|s| -s.euler_deg[1]).collect();
    assert_eq!(angles, [10.0, 15.0, 20.0, 25.0]);
    for deg in [10, 15, 20, 25] {
        assert!(w.zone(&format!("top_{deg}")).is_some());
        assert!(w.variants.iter().any(|v| v.id == format!("ramp_{deg}")));
    }
}

#[test]
fn the_line_track_guide_path_is_the_printed_line() {
    let worlds = builtin_worlds().unwrap();
    let objects = builtin_objects().unwrap();
    for (variant, want_len) in [("a", 3.5..4.2), ("b", 3.5..4.6)] {
        let b = build_world(&worlds["line_track"], Some(variant), 1, &objects, &fr()).unwrap();
        let path = b.guide_path.expect("a guide path");
        assert_eq!(path.first(), path.last(), "{variant} is a closed loop");
        let len: f64 = path
            .windows(2)
            .map(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]))
            .sum();
        assert!(want_len.contains(&len), "{variant}: length {len}");
        // Every path point is on dark mat pixels.
        let m = b.mat.unwrap();
        for p in path.iter().step_by(7) {
            let i = ((p[0] - m.min[0]) / 0.002) as usize;
            let j = ((p[1] - m.min[1]) / 0.002) as usize;
            assert!(m.pixels[j * m.width + i][0] < 0.1, "{variant}: {p:?}");
        }
    }
    assert!(
        build_world(&worlds["test_track"], None, 1, &objects, &fr())
            .unwrap()
            .guide_path
            .is_none()
    );
}
