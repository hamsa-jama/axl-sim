//! The sensors (SPEC "Sensors"): ray-cast distance and colour sensors, touch, gyro and
//! accelerometer. Each samples at its update rate into a [`DelayQueue`], adds seeded noise, and
//! has the spec's blind spots. Nothing here allocates after construction.

#![allow(clippy::needless_range_loop)]

use axl_mujoco::{GeomId, GroupMask, Physics, SensorId, SiteId};

use crate::mat::MatTexture;
use crate::noise::Noise;
use crate::params::*;
use crate::queue::{DelayQueue, VALUES};

/// The colour names the colour sensor reports, in the order of the `axl.h` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorName {
    /// Nothing in range.
    None = 0,
    /// Black.
    Black = 1,
    /// White.
    White = 2,
    /// Red.
    Red = 3,
    /// Green.
    Green = 4,
    /// Blue.
    Blue = 5,
    /// Yellow.
    Yellow = 6,
}

/// The floor mat the colour sensor reads: which geom is the mat, and its texture.
#[derive(Clone, Debug)]
pub struct FloorMat {
    /// The floor geom.
    pub geom: GeomId,
    /// Its texture, mapped by world position.
    pub texture: MatTexture,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn combine(axes: &[[f64; 3]; 3], c: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| axes[0][i] * c[0] + axes[1][i] * c[1] + axes[2][i] * c[2])
}

fn period_ms(rate_hz: f64) -> u64 {
    (1000.0 / rate_hz).round().max(1.0) as u64
}

/// Sample clock shared by the sensors: due every `period` ms.
#[derive(Clone, Copy, Debug)]
struct Clock {
    period: u64,
    next: u64,
}

impl Clock {
    fn new(rate_hz: f64) -> Self {
        let period = period_ms(rate_hz);
        Self {
            period,
            next: period,
        }
    }

    fn due(&self, now: u64) -> bool {
        now >= self.next
    }

    fn advance(&mut self, now: u64) {
        while self.next <= now {
            self.next += self.period;
        }
    }
}

/// Ray directions as local coordinates: the centre ray plus rings.
fn cone_dirs(rings: &[(f64, usize)]) -> Vec<[f64; 3]> {
    let mut v = vec![[0.0, 0.0, 1.0]];
    for &(deg, n) in rings {
        let a = deg.to_radians();
        for k in 0..n {
            let az = std::f64::consts::TAU * k as f64 / n as f64;
            v.push([a.sin() * az.cos(), a.sin() * az.sin(), a.cos()]);
        }
    }
    v
}

// ----- ultrasonic ------------------------------------------------------------------------

/// Ultrasonic distance sensor: 13 rays, nearest hit, surfaces tilted too far are missed.
#[derive(Clone, Debug)]
pub struct Ultrasonic {
    name: String,
    site: SiteId,
    p: UltrasonicParams,
    dirs: Vec<[f64; 3]>,
    min_cos: f64,
    clock: Clock,
    queue: DelayQueue,
    noise: Noise,
}

impl Ultrasonic {
    /// A sensor looking along the z axis of `site`.
    pub fn new(name: &str, site: SiteId, p: UltrasonicParams, noise: Noise) -> Self {
        Self {
            name: name.to_owned(),
            site,
            dirs: cone_dirs(&[(p.ring_deg.0, p.ring_rays), (p.ring_deg.1, p.ring_rays)]),
            min_cos: p.max_incidence_deg.to_radians().cos(),
            clock: Clock::new(p.rate_hz),
            queue: DelayQueue::new(p.delay_ms),
            p,
            noise,
        }
    }

    /// Device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// One measurement now, with noise: metres, or -1 when nothing is in range.
    pub fn measure(&mut self, physics: &mut Physics) -> f64 {
        let Some((pos, axes)) = physics.site_frame(self.site) else {
            return -1.0;
        };
        let mut best = f64::INFINITY;
        for d in &self.dirs {
            let dir = combine(&axes, *d);
            if let Some(hit) = physics.ray(pos, dir, GroupMask::WORLD_AND_OBJECTS) {
                // The surface must face the sensor within 40 degrees of its axis (decision 0012).
                let facing = -dot(hit.normal, axes[2]);
                if hit.distance >= self.p.min_m
                    && hit.distance <= self.p.max_m
                    && facing >= self.min_cos
                    && hit.distance < best
                {
                    best = hit.distance;
                }
            }
        }
        if !best.is_finite() {
            return -1.0;
        }
        let sigma = self.p.sigma_abs_m + self.p.sigma_rel * best;
        let v = best + self.noise.gauss(sigma);
        (v / self.p.resolution_m).round() * self.p.resolution_m
    }

    pub(crate) fn due(&self, now: u64) -> bool {
        self.clock.due(now)
    }

    pub(crate) fn sample(&mut self, physics: &mut Physics, now: u64) {
        let d = self.measure(physics);
        self.queue.push(now, [d, 0.0, 0.0, 0.0, 0.0, 0.0]);
        self.clock.advance(now);
    }

    /// The reading a controller sees at `now_ms`: metres, or -1.
    pub fn distance(&self, now_ms: u64) -> f64 {
        self.queue.read(now_ms).map_or(-1.0, |s| s.v[0])
    }
}

// ----- time of flight --------------------------------------------------------------------

/// Time-of-flight sensor: 5 rays within 2.5 degrees, reads surfaces up to 75 degrees off-normal.
#[derive(Clone, Debug)]
pub struct TimeOfFlight {
    name: String,
    site: SiteId,
    p: TofParams,
    dirs: Vec<[f64; 3]>,
    min_cos: f64,
    clock: Clock,
    queue: DelayQueue,
    noise: Noise,
}

impl TimeOfFlight {
    /// A sensor looking along the z axis of `site`.
    pub fn new(name: &str, site: SiteId, p: TofParams, noise: Noise) -> Self {
        Self {
            name: name.to_owned(),
            site,
            dirs: cone_dirs(&[(p.ring_deg, p.ring_rays)]),
            min_cos: p.max_incidence_deg.to_radians().cos(),
            clock: Clock::new(p.rate_hz),
            queue: DelayQueue::new(p.delay_ms),
            p,
            noise,
        }
    }

    /// Device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// One measurement now, with noise: metres, or -1. The median of the valid rays (decision 0012).
    pub fn measure(&mut self, physics: &mut Physics) -> f64 {
        let Some((pos, axes)) = physics.site_frame(self.site) else {
            return -1.0;
        };
        let mut vals = [0.0f64; 16];
        let mut n = 0usize;
        for d in self.dirs.iter().take(vals.len()) {
            let dir = combine(&axes, *d);
            if let Some(hit) = physics.ray(pos, dir, GroupMask::WORLD_AND_OBJECTS)
                && hit.distance >= self.p.min_m
                && hit.distance <= self.p.max_m
                && -dot(hit.normal, axes[2]) >= self.min_cos
            {
                vals[n] = hit.distance;
                n += 1;
            }
        }
        if n == 0 {
            return -1.0;
        }
        // The median of the valid rays: robust at edges and on tilted walls (decision 0012).
        let v = &mut vals[..n];
        v.sort_by(f64::total_cmp);
        let d = if n % 2 == 1 {
            v[n / 2]
        } else {
            0.5 * (v[n / 2 - 1] + v[n / 2])
        };
        d + self.noise.gauss(self.p.sigma_abs_m + self.p.sigma_rel * d)
    }

    pub(crate) fn due(&self, now: u64) -> bool {
        self.clock.due(now)
    }

    pub(crate) fn sample(&mut self, physics: &mut Physics, now: u64) {
        let d = self.measure(physics);
        self.queue.push(now, [d, 0.0, 0.0, 0.0, 0.0, 0.0]);
        self.clock.advance(now);
    }

    /// The reading a controller sees at `now_ms`: metres, or -1.
    pub fn distance(&self, now_ms: u64) -> f64 {
        self.queue.read(now_ms).map_or(-1.0, |s| s.v[0])
    }
}

// ----- colour ----------------------------------------------------------------------------

/// What the colour sensor reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorReading {
    /// Red, green, blue, 0 to full scale (1023).
    pub rgb: [i32; 3],
    /// Reflectance, 0 to 100.
    pub reflect: f64,
    /// Nearest colour name.
    pub name: ColorName,
}

/// Colour sensor: five samples over a 3 mm spot, intensity falling off with height.
#[derive(Clone, Debug)]
pub struct ColorSensor {
    name: String,
    site: SiteId,
    p: ColorParams,
    refs: [(ColorName, [f64; 3]); 6],
    clock: Clock,
    queue: DelayQueue,
    noise: Noise,
}

impl ColorSensor {
    /// A sensor looking along the z axis of `site`.
    pub fn new(name: &str, site: SiteId, p: ColorParams, names: &ColorNames, noise: Noise) -> Self {
        Self {
            name: name.to_owned(),
            site,
            refs: [
                (ColorName::Black, names.black),
                (ColorName::White, names.white),
                (ColorName::Red, names.red),
                (ColorName::Green, names.green),
                (ColorName::Blue, names.blue),
                (ColorName::Yellow, names.yellow),
            ],
            clock: Clock::new(p.rate_hz),
            queue: DelayQueue::new(p.delay_ms),
            p,
            noise,
        }
    }

    /// Device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Intensity factor at distance `d`: 1 / (1 + ((d - 8 mm) / 15 mm)^2).
    pub fn intensity(&self, d: f64) -> f64 {
        let x = (d - self.p.peak_m) / self.p.width_m;
        1.0 / (1.0 + x * x)
    }

    fn nearest(&self, c: [f64; 3]) -> ColorName {
        let mut best = (f64::INFINITY, ColorName::None);
        for (n, r) in &self.refs {
            let d = (c[0] - r[0]).powi(2) + (c[1] - r[1]).powi(2) + (c[2] - r[2]).powi(2);
            if d < best.0 {
                best = (d, *n);
            }
        }
        best.1
    }

    /// One measurement now, with noise.
    pub fn measure(&mut self, physics: &mut Physics, mat: Option<&FloorMat>) -> ColorReading {
        let none = ColorReading {
            rgb: [0; 3],
            reflect: 0.0,
            name: ColorName::None,
        };
        let Some((pos, axes)) = physics.site_frame(self.site) else {
            return none;
        };
        let r = self.p.spot_radius_m;
        let offsets = [[0.0, 0.0], [r, 0.0], [-r, 0.0], [0.0, r], [0.0, -r]];
        let (mut lit, mut plain, mut hits) = ([0.0; 3], [0.0; 3], 0usize);
        for o in offsets {
            let from: [f64; 3] =
                std::array::from_fn(|i| pos[i] + axes[0][i] * o[0] + axes[1][i] * o[1]);
            let Some(hit) = physics.ray(from, axes[2], GroupMask::WORLD_AND_OBJECTS) else {
                continue;
            };
            if hit.distance < self.p.min_m || hit.distance > self.p.max_m {
                continue;
            }
            let col: [f64; 3] = match mat {
                Some(m) if m.geom == hit.geom => match m.texture.sample(hit.point[0], hit.point[1])
                {
                    Some(c) => [c[0] as f64, c[1] as f64, c[2] as f64],
                    None => rgba3(physics, hit.geom),
                },
                _ => rgba3(physics, hit.geom),
            };
            let f = self.intensity(hit.distance);
            for i in 0..3 {
                lit[i] += col[i] * f;
                plain[i] += col[i];
            }
            hits += 1;
        }
        if hits == 0 {
            // Nothing in range: dark, with the sensor's noise floor.
            let fs = self.p.full_scale;
            let rgb = std::array::from_fn(|_| {
                (self.noise.gauss(self.p.sigma_frac * fs))
                    .round()
                    .clamp(0.0, fs) as i32
            });
            return ColorReading { rgb, ..none };
        }
        let fs = self.p.full_scale;
        let rgb: [i32; 3] = std::array::from_fn(|i| {
            let v = lit[i] / 5.0 * fs + self.noise.gauss(self.p.sigma_frac * fs);
            v.round().clamp(0.0, fs) as i32
        });
        let reflect = (rgb[0] + rgb[1] + rgb[2]) as f64 / 3.0 / fs * 100.0;
        let mean = [
            plain[0] / hits as f64,
            plain[1] / hits as f64,
            plain[2] / hits as f64,
        ];
        ColorReading {
            rgb,
            reflect,
            name: self.nearest(mean),
        }
    }

    pub(crate) fn due(&self, now: u64) -> bool {
        self.clock.due(now)
    }

    pub(crate) fn sample(&mut self, physics: &mut Physics, now: u64, mat: Option<&FloorMat>) {
        let r = self.measure(physics, mat);
        self.queue.push(
            now,
            [
                r.rgb[0] as f64,
                r.rgb[1] as f64,
                r.rgb[2] as f64,
                r.reflect,
                r.name as i32 as f64,
                0.0,
            ],
        );
        self.clock.advance(now);
    }

    /// The reading a controller sees at `now_ms`.
    pub fn reading(&self, now_ms: u64) -> ColorReading {
        match self.queue.read(now_ms) {
            Some(s) => ColorReading {
                rgb: [s.v[0] as i32, s.v[1] as i32, s.v[2] as i32],
                reflect: s.v[3],
                name: match s.v[4] as i32 {
                    1 => ColorName::Black,
                    2 => ColorName::White,
                    3 => ColorName::Red,
                    4 => ColorName::Green,
                    5 => ColorName::Blue,
                    6 => ColorName::Yellow,
                    _ => ColorName::None,
                },
            },
            None => ColorReading {
                rgb: [0; 3],
                reflect: 0.0,
                name: ColorName::None,
            },
        }
    }
}

fn rgba3(physics: &Physics, g: GeomId) -> [f64; 3] {
    let c = physics.geom_rgba(g);
    [c[0] as f64, c[1] as f64, c[2] as f64]
}

// ----- touch -----------------------------------------------------------------------------

/// Touch sensor: pressed above 1.0 N, debounced over 5 ms.
#[derive(Clone, Debug)]
pub struct TouchSensor {
    name: String,
    sensor: SensorId,
    p: TouchParams,
    clock: Clock,
    queue: DelayQueue,
    noise: Noise,
    pressed: bool,
    counter: u32,
}

impl TouchSensor {
    /// A touch sensor reading MuJoCo's `touch` sensor `sensor`.
    pub fn new(name: &str, sensor: SensorId, p: TouchParams, noise: Noise) -> Self {
        Self {
            name: name.to_owned(),
            sensor,
            clock: Clock::new(p.rate_hz),
            queue: DelayQueue::new(p.delay_ms),
            p,
            noise,
            pressed: false,
            counter: 0,
        }
    }

    /// Device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn force(&self, physics: &Physics) -> f64 {
        physics.sensor(self.sensor).first().copied().unwrap_or(0.0)
    }

    /// Every 1 ms step: debounce the raw force. The state changes only after the opposite state
    /// has held for the debounce time.
    pub fn step(&mut self, physics: &Physics) {
        let raw = self.force(physics) > self.p.threshold_n;
        if raw == self.pressed {
            self.counter = 0;
        } else {
            self.counter += 1;
            if self.counter >= self.p.debounce_ms {
                self.pressed = raw;
                self.counter = 0;
            }
        }
    }

    /// The debounced state right now (no delay).
    pub fn pressed_now(&self) -> bool {
        self.pressed
    }

    /// One force measurement now, with noise, limited to 0..10 N.
    pub fn measure(&mut self, physics: &Physics) -> f64 {
        (self.force(physics) + self.noise.gauss(self.p.sigma_n)).clamp(0.0, self.p.max_n)
    }

    pub(crate) fn due(&self, now: u64) -> bool {
        self.clock.due(now)
    }

    pub(crate) fn sample(&mut self, physics: &Physics, now: u64) {
        let f = self.measure(physics);
        let pr = if self.pressed { 1.0 } else { 0.0 };
        self.queue.push(now, [f, pr, 0.0, 0.0, 0.0, 0.0]);
        self.clock.advance(now);
    }

    /// Force in newtons as a controller sees it at `now_ms`.
    pub fn force_reading(&self, now_ms: u64) -> f64 {
        self.queue.read(now_ms).map_or(0.0, |s| s.v[0])
    }

    /// Pressed state as a controller sees it at `now_ms`.
    pub fn pressed(&self, now_ms: u64) -> bool {
        self.queue.read(now_ms).is_some_and(|s| s.v[1] > 0.5)
    }
}

// ----- hub IMU ---------------------------------------------------------------------------

fn quantise(v: f64, range: f64, bits: u32) -> f64 {
    let lsb = 2.0 * range / (1u64 << bits) as f64;
    (v.clamp(-range, range) / lsb).round() * lsb
}

/// The hub's gyroscope and accelerometer (robot frame, read from MuJoCo sensors on site `imu`).
#[derive(Clone, Debug)]
pub struct Imu {
    gyro_sensor: SensorId,
    accel_sensor: SensorId,
    gp: GyroParams,
    ap: AccelParams,
    gyro_clock: Clock,
    accel_clock: Clock,
    gyro_q: DelayQueue,
    accel_q: DelayQueue,
    gyro_noise: Noise,
    accel_noise: Noise,
    gyro_bias: [f64; 3],
    accel_bias: [f64; 3],
}

impl Imu {
    /// An IMU. The biases are drawn now from the noise streams.
    pub fn new(
        gyro_sensor: SensorId,
        accel_sensor: SensorId,
        gp: GyroParams,
        ap: AccelParams,
        mut gyro_noise: Noise,
        mut accel_noise: Noise,
    ) -> Self {
        let gyro_bias = std::array::from_fn(|_| gyro_noise.gauss(gp.bias_sigma));
        let accel_bias = std::array::from_fn(|_| accel_noise.gauss(ap.bias_sigma));
        Self {
            gyro_sensor,
            accel_sensor,
            gyro_clock: Clock::new(gp.rate_hz),
            accel_clock: Clock::new(ap.rate_hz),
            gyro_q: DelayQueue::new(gp.delay_ms),
            accel_q: DelayQueue::new(ap.delay_ms),
            gp,
            ap,
            gyro_noise,
            accel_noise,
            gyro_bias,
            accel_bias,
        }
    }

    /// The present gyro bias per axis, rad/s.
    pub fn gyro_bias(&self) -> [f64; 3] {
        self.gyro_bias
    }

    /// The accelerometer bias per axis, m/s2.
    pub fn accel_bias(&self) -> [f64; 3] {
        self.accel_bias
    }

    /// One gyro measurement now, rad/s. The bias drifts by a random step each call.
    pub fn measure_gyro(&mut self, physics: &Physics) -> [f64; 3] {
        let truth = physics.sensor(self.gyro_sensor);
        let mut out = [0.0; 3];
        for k in 0..3 {
            self.gyro_bias[k] += self.gyro_noise.gauss(self.gp.drift_per_sample);
            let t = truth.get(k).copied().unwrap_or(0.0);
            let v = t + self.gyro_bias[k] + self.gyro_noise.gauss(self.gp.sigma);
            out[k] = quantise(v, self.gp.range, self.gp.bits);
        }
        out
    }

    /// One accelerometer measurement now, m/s2.
    pub fn measure_accel(&mut self, physics: &Physics) -> [f64; 3] {
        let truth = physics.sensor(self.accel_sensor);
        let mut out = [0.0; 3];
        for k in 0..3 {
            let t = truth.get(k).copied().unwrap_or(0.0);
            let v = t + self.accel_bias[k] + self.accel_noise.gauss(self.ap.sigma);
            out[k] = quantise(v, self.ap.range, self.ap.bits);
        }
        out
    }

    pub(crate) fn gyro_due(&self, now: u64) -> bool {
        self.gyro_clock.due(now)
    }

    pub(crate) fn accel_due(&self, now: u64) -> bool {
        self.accel_clock.due(now)
    }

    pub(crate) fn sample_gyro(&mut self, physics: &Physics, now: u64) {
        let g = self.measure_gyro(physics);
        let mut v = [0.0; VALUES];
        v[..3].copy_from_slice(&g);
        self.gyro_q.push(now, v);
        self.gyro_clock.advance(now);
    }

    pub(crate) fn sample_accel(&mut self, physics: &Physics, now: u64) {
        let a = self.measure_accel(physics);
        let mut v = [0.0; VALUES];
        v[..3].copy_from_slice(&a);
        self.accel_q.push(now, v);
        self.accel_clock.advance(now);
    }

    /// Gyro as a controller sees it at `now_ms`, rad/s.
    pub fn gyro(&self, now_ms: u64) -> [f64; 3] {
        self.gyro_q
            .read(now_ms)
            .map_or([0.0; 3], |s| [s.v[0], s.v[1], s.v[2]])
    }

    /// Accelerometer as a controller sees it at `now_ms`, m/s2.
    pub fn accel(&self, now_ms: u64) -> [f64; 3] {
        self.accel_q
            .read(now_ms)
            .map_or([0.0; 3], |s| [s.v[0], s.v[1], s.v[2]])
    }
}
