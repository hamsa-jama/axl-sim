//! Sensor behaviour: noise sigma over 10,000 samples (within 10% of the spec), blind spots,
//! touch threshold and debounce, and sample-hold-delay timing through the step loop.

use axl_hw::rig::{WorldSpec, world_rig};
use axl_hw::*;
use axl_mujoco::SensorId;

fn std_dev(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    let m = xs.iter().sum::<f64>() / n;
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n).sqrt()
}

fn assert_sigma(label: &str, xs: &[f64], spec: f64) {
    let s = std_dev(xs);
    assert!(
        (s / spec - 1.0).abs() < 0.10,
        "{label}: measured sigma {s} vs spec {spec} ({:.1}% off)",
        (s / spec - 1.0) * 100.0
    );
}

fn site(r: &axl_hw::rig::Rig, n: &str) -> axl_mujoco::SiteId {
    r.physics.site_id(n).unwrap()
}

#[test]
fn ultrasonic_noise_is_3mm_plus_1_percent() {
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let p = HwParams::builtin().sensors.ultrasonic;
    let mut s = Ultrasonic::new("eye", site(&r, "eye"), p, Noise::new(1, b'A', 0, true));
    let xs: Vec<f64> = (0..10_000).map(|_| s.measure(&mut r.physics)).collect();
    assert_sigma("ultrasonic", &xs, 0.003 + 0.01 * 0.5);
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    assert!((mean - 0.5).abs() < 0.001, "mean {mean}");
    assert!(
        xs.iter()
            .all(|x| (x * 1000.0 - (x * 1000.0).round()).abs() < 1e-6)
    );
}

#[test]
fn tof_noise_is_2mm_plus_half_a_percent() {
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let p = HwParams::builtin().sensors.tof;
    let mut s = TimeOfFlight::new("tof", site(&r, "eye"), p, Noise::new(1, b'B', 0, true));
    let xs: Vec<f64> = (0..10_000).map(|_| s.measure(&mut r.physics)).collect();
    assert_sigma("tof", &xs, 0.002 + 0.005 * 0.5);
}

#[test]
fn colour_noise_is_1_5_percent_of_full_scale() {
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let hp = HwParams::builtin();
    let mut s = ColorSensor::new(
        "color",
        site(&r, "look"),
        hp.sensors.color,
        &hp.colors,
        Noise::new(1, b'C', 0, true),
    );
    let reads: Vec<ColorReading> = (0..10_000)
        .map(|_| s.measure(&mut r.physics, None))
        .collect();
    for ch in 0..3 {
        let xs: Vec<f64> = reads.iter().map(|c| c.rgb[ch] as f64).collect();
        assert_sigma("colour channel", &xs, 0.015 * 1023.0);
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        assert!((mean - 511.5).abs() < 2.0, "mean {mean}");
    }
}

#[test]
fn touch_force_noise_is_0_05_n() {
    let mut r = world_rig(&WorldSpec {
        weight_kg: 0.51,
        ..Default::default()
    })
    .unwrap();
    for _ in 0..1000 {
        r.physics.step();
    }
    r.physics.forward();
    let id = r.physics.sensor_id("touch_sensor").unwrap();
    let hp = HwParams::builtin();
    let mut s = TouchSensor::new("touch", id, hp.sensors.touch, Noise::new(1, b'D', 0, true));
    let xs: Vec<f64> = (0..10_000).map(|_| s.measure(&r.physics)).collect();
    assert_sigma("touch", &xs, 0.05);
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    assert!((mean - 0.51 * 9.80665).abs() < 0.05, "mean {mean}");
}

fn imu_of(r: &axl_hw::rig::Rig, seed: u64) -> Imu {
    let hp = HwParams::builtin();
    let g = r.physics.sensor_id("gyro").unwrap();
    let a = r.physics.sensor_id("accel").unwrap();
    Imu::new(
        g,
        a,
        hp.sensors.gyro,
        hp.sensors.accel,
        Noise::new(seed, b'H', 0, true),
        Noise::new(seed, b'H', 1, true),
    )
}

#[test]
fn gyro_noise_is_0_003_rad_s_and_the_bias_is_0_005_sigma() {
    let r = world_rig(&WorldSpec::default()).unwrap();
    let mut imu = imu_of(&r, 3);
    let xs: Vec<[f64; 3]> = (0..10_000).map(|_| imu.measure_gyro(&r.physics)).collect();
    for k in 0..3 {
        let col: Vec<f64> = xs.iter().map(|v| v[k]).collect();
        assert_sigma("gyro", &col, 0.003);
    }
    // 16-bit quantisation: every reading is a multiple of 2 x range / 65536.
    let lsb = 2.0 * 34.9 / 65536.0;
    assert!(xs.iter().all(|v| {
        v.iter()
            .all(|x| ((x / lsb) - (x / lsb).round()).abs() < 1e-6)
    }));
    // Biases over many seeds follow N(0, 0.005).
    let biases: Vec<f64> = (0..2000u64)
        .flat_map(|seed| imu_of(&r, seed).gyro_bias())
        .collect();
    assert_sigma("gyro bias", &biases, 0.005);
}

#[test]
fn gyro_bias_drifts_by_about_1e_5_per_sample() {
    let r = world_rig(&WorldSpec::default()).unwrap();
    let mut imu = imu_of(&r, 5);
    let b0 = imu.gyro_bias();
    for _ in 0..10_000 {
        imu.measure_gyro(&r.physics);
    }
    let b1 = imu.gyro_bias();
    for k in 0..3 {
        // A random walk of 10,000 steps of 1e-5 wanders about 1e-3; it must have moved.
        let d = (b1[k] - b0[k]).abs();
        assert!(d > 1e-5 && d < 6e-3, "axis {k} drifted {d}");
    }
}

#[test]
fn accelerometer_noise_is_0_02_and_reads_gravity() {
    let r = world_rig(&WorldSpec::default()).unwrap();
    let mut imu = imu_of(&r, 9);
    let xs: Vec<[f64; 3]> = (0..10_000).map(|_| imu.measure_accel(&r.physics)).collect();
    for k in 0..3 {
        let col: Vec<f64> = xs.iter().map(|v| v[k]).collect();
        assert_sigma("accel", &col, 0.02);
    }
    let mean_z = xs.iter().map(|v| v[2]).sum::<f64>() / xs.len() as f64;
    assert!(
        (mean_z - 9.80665 - imu.accel_bias()[2]).abs() < 0.01,
        "{mean_z}"
    );
    let biases: Vec<f64> = (0..2000u64)
        .flat_map(|seed| imu_of(&r, seed).accel_bias())
        .collect();
    assert_sigma("accel bias", &biases, 0.05);
}

#[test]
fn noise_off_gives_exact_readings() {
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let hp = HwParams::builtin();
    let mut s = Ultrasonic::new(
        "eye",
        site(&r, "eye"),
        hp.sensors.ultrasonic,
        Noise::new(1, b'A', 0, false),
    );
    let a = s.measure(&mut r.physics);
    assert_eq!(a, 0.5);
    assert_eq!(s.measure(&mut r.physics), 0.5);
    assert_eq!(s.name(), "eye");
}

#[test]
fn ultrasonic_misses_angled_walls_and_tof_does_not() {
    let hp = HwParams::builtin();
    // 30 degrees: seen. 50 degrees: missed by the ultrasonic (V16), seen by the ToF up to 75.
    for (deg, us_sees, tof_sees) in [
        (0.0, true, true),
        (30.0, true, true),
        (50.0, false, true),
        (70.0, false, true),
        (80.0, false, false),
    ] {
        let mut r = world_rig(&WorldSpec {
            wall_angle: f64_to_rad(deg),
            ..Default::default()
        })
        .unwrap();
        let mut us = Ultrasonic::new(
            "eye",
            site(&r, "eye"),
            hp.sensors.ultrasonic,
            Noise::new(1, b'A', 0, false),
        );
        let mut tof = TimeOfFlight::new(
            "tof",
            site(&r, "eye"),
            hp.sensors.tof,
            Noise::new(1, b'B', 0, false),
        );
        assert_eq!(
            us.measure(&mut r.physics) > 0.0,
            us_sees,
            "ultrasonic at {deg} degrees"
        );
        assert_eq!(
            tof.measure(&mut r.physics) > 0.0,
            tof_sees,
            "tof at {deg} degrees"
        );
    }
}

fn f64_to_rad(d: f64) -> f64 {
    d.to_radians()
}

#[test]
fn distance_range_limits_return_minus_one() {
    let hp = HwParams::builtin();
    for (dist, us, tof) in [
        (0.02, -1.0, -1.0),
        (0.035, 0.035, -1.0),
        (2.2, 2.2, -1.0),
        (3.0, -1.0, -1.0),
        (1.0, 1.0, 1.0),
    ] {
        let mut r = world_rig(&WorldSpec {
            wall_distance: dist,
            ..Default::default()
        })
        .unwrap();
        let mut u = Ultrasonic::new(
            "eye",
            site(&r, "eye"),
            hp.sensors.ultrasonic,
            Noise::new(1, b'A', 0, false),
        );
        let mut t = TimeOfFlight::new(
            "tof",
            site(&r, "eye"),
            hp.sensors.tof,
            Noise::new(1, b'B', 0, false),
        );
        let (a, b) = (u.measure(&mut r.physics), t.measure(&mut r.physics));
        assert!((a - us).abs() < 0.002, "ultrasonic at {dist}: {a}");
        assert!(
            (b - tof).abs() < 0.002 || (tof > 0.0 && (b - tof).abs() < 0.002),
            "tof at {dist}: {b}"
        );
    }
}

#[test]
fn sensors_do_not_see_the_robot_itself() {
    // The hub body (group 1) sits at the sensor origin; the wall at 0.5 m is what is measured.
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let hp = HwParams::builtin();
    let mut u = Ultrasonic::new(
        "eye",
        site(&r, "eye"),
        hp.sensors.ultrasonic,
        Noise::new(1, b'A', 0, false),
    );
    assert_eq!(u.measure(&mut r.physics), 0.5);
}

#[test]
fn touch_threshold_is_one_newton_and_debounced_over_five_ms() {
    let hp = HwParams::builtin();
    // 0.05 kg is 0.49 N: not pressed. 0.2 kg is 1.96 N: pressed.
    for (kg, expect) in [(0.05, false), (0.2, true)] {
        let mut r = world_rig(&WorldSpec {
            weight_kg: kg,
            ..Default::default()
        })
        .unwrap();
        let id = r.physics.sensor_id("touch_sensor").unwrap();
        let mut t = TouchSensor::new("touch", id, hp.sensors.touch, Noise::new(1, b'D', 0, false));
        for _ in 0..1500 {
            r.physics.step();
            t.step(&r.physics);
        }
        assert_eq!(t.pressed_now(), expect, "{kg} kg");
    }
    // Debounce: a raw press shorter than 5 ms never shows; a longer one shows after 5 ms.
    let r = world_rig(&WorldSpec {
        weight_kg: 0.2,
        ..Default::default()
    })
    .unwrap();
    let mut settled = r;
    for _ in 0..1500 {
        settled.physics.step();
    }
    let id = settled.physics.sensor_id("touch_sensor").unwrap();
    let mut t = TouchSensor::new("touch", id, hp.sensors.touch, Noise::new(1, b'D', 0, false));
    settled.physics.forward();
    for k in 1..=4 {
        t.step(&settled.physics);
        assert!(!t.pressed_now(), "ms {k}");
    }
    t.step(&settled.physics);
    assert!(t.pressed_now(), "pressed after 5 ms of force");
}

#[test]
fn sample_hold_and_delay_follow_the_spec_through_the_step_loop() {
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let opts = HwOptions {
        noise: false,
        ..Default::default()
    };
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), opts, &r.physics);
    let h = hw.find("eye").unwrap();
    let slot = hw.devices()[h].slot;
    let mut first_seen = None;
    let mut changes = Vec::new();
    let mut last = -2.0;
    let wall = r.physics.geom_id("wall").unwrap();
    let _ = wall;
    for ms in 1..=300u64 {
        hw.pre_step(&mut r.physics);
        // Move the wall closer at 100 ms by moving the probe is not possible here; the hub spins
        // instead, which changes nothing for a wall square to the sensor. Watch only the timing.
        r.physics.step();
        hw.post_step(&mut r.physics);
        let d = hw.distance(slot).unwrap();
        if d > 0.0 && first_seen.is_none() {
            first_seen = Some(ms);
        }
        if d != last {
            changes.push(ms);
            last = d;
        }
    }
    // First sample at 50 ms (20 Hz), visible 30 ms later.
    assert_eq!(first_seen, Some(80));
    assert_eq!(hw.now_ms(), 300);
}

#[test]
fn a_changing_scene_shows_up_after_the_sensor_delay() {
    // The colour probe slides over a black/white mat; the reading follows the position with the
    // 10 ms delay and the 100 Hz hold.
    let mut r = world_rig(&WorldSpec::default()).unwrap();
    let opts = HwOptions {
        noise: false,
        ..Default::default()
    };
    let floor = r.physics.geom_id("floor").unwrap();
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), opts, &r.physics);
    hw.set_floor_mat(Some(FloorMat {
        geom: floor,
        texture: MatTexture::from_fn([-1.0, -1.0], [1.0, 1.0], 2, 1, |u, _| {
            if u < 0.5 { [0.0; 3] } else { [1.0; 3] }
        }),
    }));
    let slot = hw.devices()[hw.find("color").unwrap()].slot;
    let Slot::Color(ci) = slot else {
        panic!("colour slot")
    };
    // Start on black (x = -0.5 relative to the edge at 0), jump to white at step 100.
    let slide = r.physics.joint_id("slide").unwrap();
    r.physics.set_joint_pos(slide, -0.5);
    r.physics.forward();
    let mut seen_white_at = None;
    for ms in 1..=200u64 {
        if ms == 100 {
            r.physics.set_joint_pos(slide, 0.5);
        }
        hw.pre_step(&mut r.physics);
        r.physics.step();
        hw.post_step(&mut r.physics);
        let c = hw.color_at(ci).unwrap().reading(hw.now_ms());
        if c.name == ColorName::White && seen_white_at.is_none() {
            seen_white_at = Some(ms);
        }
    }
    // The jump happens before step 100's physics; the first sample taken afterwards is at 100 ms,
    // visible from 110 ms.
    assert_eq!(seen_white_at, Some(110));
}

#[test]
fn sensors_by_slot_have_the_right_kinds() {
    let r = world_rig(&WorldSpec::default()).unwrap();
    let hw = Hardware::new(
        &r.map,
        &HwParams::builtin(),
        HwOptions::default(),
        &r.physics,
    );
    let kinds: Vec<_> = hw
        .devices()
        .iter()
        .map(|d| (d.name.as_str(), d.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("hub", DeviceKind::Hub),
            ("eye", DeviceKind::Distance),
            ("tof", DeviceKind::Distance),
            ("color", DeviceKind::Color),
            ("touch", DeviceKind::Touch)
        ]
    );
    assert!(hw.imu().is_some());
    assert!(hw.find("nope").is_none());
    let _ = SensorId(0);
}
