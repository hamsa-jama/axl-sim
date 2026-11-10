//! Checking criteria against a trace.

use axl_world::Zone;

use crate::def::{Criterion, Expected, MissionDef};
use crate::trace::{RunTrace, Sample};

/// A robot counts as stopped below this speed, m/s.
pub const STOPPED_MS: f64 = 0.02;

/// The world's data the criteria need.
#[derive(Clone, Debug, Default)]
pub struct EvalContext {
    /// Grading zones.
    pub zones: Vec<Zone>,
    /// The guide line (closed loop) for `FollowLine`.
    pub line_path: Option<Vec<[f64; 2]>>,
}

/// The verdict on one criterion.
#[derive(Clone, Debug, PartialEq)]
pub struct CriterionResult {
    /// Short name.
    pub name: String,
    /// Whether it holds.
    pub ok: bool,
    /// What happened, with numbers: "Stopped 38 mm too far (front at 1.088 m, wanted 0.95 to 1.05)".
    pub message: String,
}

/// The verdict for one seed.
#[derive(Clone, Debug, PartialEq)]
pub struct SeedResult {
    /// The seed.
    pub seed: u64,
    /// One entry per criterion.
    pub results: Vec<CriterionResult>,
}

impl SeedResult {
    /// All criteria hold.
    pub fn passed(&self) -> bool {
        self.results.iter().all(|r| r.ok)
    }
}

fn wrap(a: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    (a + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn xy(s: &Sample) -> [f64; 2] {
    [s.pos[0], s.pos[1]]
}

fn mm(m: f64) -> String {
    format!("{:.0} mm", m * 1000.0)
}

fn deg(r: f64) -> String {
    format!("{:.1} degrees", r.to_degrees())
}

fn res(name: &str, ok: bool, message: String) -> CriterionResult {
    CriterionResult {
        name: name.to_owned(),
        ok,
        message,
    }
}

/// Distance from `p` to the closed polyline, and the arc length of the nearest point.
fn nearest_on_loop(path: &[[f64; 2]], p: [f64; 2]) -> (f64, f64) {
    let mut best = (f64::MAX, 0.0);
    let mut acc = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
        let len2 = abx * abx + aby * aby;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / len2).clamp(0.0, 1.0)
        };
        let q = [a[0] + t * abx, a[1] + t * aby];
        let d = dist(p, q);
        if d < best.0 {
            best = (d, acc + t * len2.sqrt());
        }
        acc += len2.sqrt();
    }
    best
}

fn loop_length(path: &[[f64; 2]]) -> f64 {
    path.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Check one criterion.
pub fn check(c: &Criterion, ctx: &EvalContext, t: &RunTrace) -> CriterionResult {
    let Some(first) = t.first() else {
        return res("run", false, "The run recorded nothing.".to_owned());
    };
    let last = t.last().unwrap_or(first);
    match c {
        Criterion::ReachZone { zone, within_s } | Criterion::FinishWithin { zone, within_s } => {
            let Some(z) = ctx.zones.iter().find(|z| &z.name == zone) else {
                return res("zone", false, format!("The world has no zone {zone}."));
            };
            let hit = t
                .samples
                .iter()
                .find(|s| s.t <= *within_s && z.contains(xy(s)));
            match hit {
                Some(s) => res(
                    "reach zone",
                    true,
                    format!("Reached {zone} at {:.1} s (limit {within_s} s).", s.t),
                ),
                None => {
                    let late = t.samples.iter().find(|s| z.contains(xy(s)));
                    let msg = match late {
                        Some(s) => format!(
                            "Reached {zone} at {:.1} s, which is {:.1} s too late (limit {within_s} s).",
                            s.t,
                            s.t - within_s
                        ),
                        None => format!("Never reached {zone} within {within_s} s."),
                    };
                    res("reach zone", false, msg)
                }
            }
        }
        Criterion::FinalPose {
            x,
            y,
            yaw,
            pos_tol_m,
            yaw_tol_rad,
        } => {
            let dp = dist(xy(last), [*x, *y]);
            let dy = wrap(last.yaw - yaw).abs();
            let ok = dp <= *pos_tol_m && dy <= *yaw_tol_rad;
            res(
                "final pose",
                ok,
                format!(
                    "Ended {} from the target position (allowed {}) and {} off in heading (allowed {}).",
                    mm(dp),
                    mm(*pos_tol_m),
                    deg(dy),
                    deg(*yaw_tol_rad)
                ),
            )
        }
        Criterion::FinalX { x, tol_m, within_s } => {
            let err = last.pos[0] - x;
            let stopped = last.speed < STOPPED_MS;
            let mut ok = err.abs() <= *tol_m && stopped;
            let mut msg = format!(
                "Ended at x = {:.3} m, {} {} (allowed {}).{}",
                last.pos[0],
                mm(err.abs()),
                if err >= 0.0 { "too far" } else { "short" },
                mm(*tol_m),
                if stopped {
                    ""
                } else {
                    " The robot was still moving."
                }
            );
            if let (true, Some(limit)) = (ok, within_s) {
                // The last time the robot was outside the tolerance.
                let settled = t
                    .samples
                    .iter()
                    .rev()
                    .find(|s| (s.pos[0] - x).abs() > *tol_m)
                    .map_or(0.0, |s| s.t);
                if settled > *limit {
                    ok = false;
                    msg = format!(
                        "Got to x = {x:.3} m and stayed there only from {settled:.1} s; the limit is {limit} s."
                    );
                }
            }
            res("final x", ok, msg)
        }
        Criterion::FrontStop {
            min_x,
            max_x,
            within_s,
        } => {
            let s = t.at(*within_s).unwrap_or(last);
            let fx = s.front[0];
            let in_range = fx >= *min_x && fx <= *max_x;
            let stopped = s.speed < STOPPED_MS;
            let msg = if !stopped {
                format!(
                    "At {within_s} s the robot was still moving ({:.2} m/s); its front was at {fx:.3} m.",
                    s.speed
                )
            } else if in_range {
                format!("The front stopped at {fx:.3} m (wanted {min_x} to {max_x} m).")
            } else if fx < *min_x {
                format!(
                    "Stopped {} short: the front was at {fx:.3} m (wanted {min_x} to {max_x} m).",
                    mm(min_x - fx)
                )
            } else {
                format!(
                    "Stopped {} too far: the front was at {fx:.3} m (wanted {min_x} to {max_x} m).",
                    mm(fx - max_x)
                )
            };
            res("front stop", in_range && stopped, msg)
        }
        Criterion::TurnTo {
            yaw,
            tol_rad,
            drift_m,
        } => {
            let turned = last.yaw - first.yaw;
            let err = (turned - yaw).abs();
            let drift = dist(last.pivot, first.pivot);
            let ok = err <= *tol_rad && drift <= *drift_m && last.speed < STOPPED_MS;
            res(
                "turn",
                ok,
                format!(
                    "Turned {} in total, {} from the target {} (allowed {}); the point between the wheels drifted {} (allowed {}).",
                    deg(turned),
                    deg(err),
                    deg(*yaw),
                    deg(*tol_rad),
                    mm(drift),
                    mm(*drift_m)
                ),
            )
        }
        Criterion::Waypoints {
            points,
            tol_m,
            end_tol_m,
        } => {
            let start = xy(first);
            let mut next = 0;
            for s in &t.samples {
                if next < points.len()
                    && dist(
                        xy(s),
                        [start[0] + points[next][0], start[1] + points[next][1]],
                    ) <= *tol_m
                {
                    next += 1;
                }
            }
            let end = dist(xy(last), start);
            let ok = next == points.len() && end <= *end_tol_m;
            let msg = if next < points.len() {
                format!(
                    "Reached {next} of {} corners (within {}); corner {} was missed.",
                    points.len(),
                    mm(*tol_m),
                    next + 1
                )
            } else {
                format!(
                    "All {} corners visited; ended {} from the start (allowed {}).",
                    points.len(),
                    mm(end),
                    mm(*end_tol_m)
                )
            };
            res("square", ok, msg)
        }
        Criterion::NoContact { tag } => match t.samples.iter().find(|s| s.wall_contact) {
            Some(s) => res(
                "no contact",
                false,
                format!("Touched a {tag} at {:.2} s.", s.t),
            ),
            None => res("no contact", true, format!("Never touched a {tag}.")),
        },
        Criterion::FrontClearance {
            min_m,
            max_m,
            within_s,
        } => {
            let s = t.at(*within_s).unwrap_or(last);
            let d = s.ahead_m;
            let stopped = s.speed < STOPPED_MS;
            let ok = d >= *min_m && d <= *max_m && stopped;
            let msg = if !stopped {
                format!("At {within_s} s the robot was still moving.")
            } else if d < 0.0 {
                "Nothing ahead of the robot to measure against.".to_owned()
            } else {
                format!(
                    "Stopped {} from the wall (wanted {} to {}).",
                    mm(d),
                    mm(*min_m),
                    mm(*max_m)
                )
            };
            res("distance to the wall", ok, msg)
        }
        Criterion::FollowLine {
            max_dev_m,
            laps,
            within_s,
        } => {
            let Some(path) = ctx.line_path.as_deref().filter(|p| p.len() > 1) else {
                return res("line", false, "The world has no guide line.".to_owned());
            };
            let len = loop_length(path);
            let mut progress = 0.0;
            let mut prev: Option<f64> = None;
            let mut max_dev = 0.0_f64;
            let mut done_at = None;
            let need = f64::from(*laps) * len * 0.97;
            for s in &t.samples {
                let (d, arc) = nearest_on_loop(path, xy(s));
                if let Some(p) = prev {
                    let mut step = arc - p;
                    if step > len / 2.0 {
                        step -= len;
                    } else if step < -len / 2.0 {
                        step += len;
                    }
                    progress += step;
                }
                prev = Some(arc);
                if done_at.is_none() {
                    max_dev = max_dev.max(d);
                    if progress.abs() >= need {
                        done_at = Some(s.t);
                    }
                }
            }
            let lap_text = format!(
                "{:.0}% of the lap",
                (progress.abs() / len * 100.0).min(999.0)
            );
            let (ok, msg) = match done_at {
                Some(tt) if tt <= *within_s && max_dev <= *max_dev_m => (
                    true,
                    format!(
                        "Lap in {tt:.1} s (limit {within_s} s); widest miss {} (allowed {}).",
                        mm(max_dev),
                        mm(*max_dev_m)
                    ),
                ),
                Some(tt) if tt > *within_s => (
                    false,
                    format!("The lap took {tt:.1} s; the limit is {within_s} s."),
                ),
                Some(_) => (
                    false,
                    format!(
                        "Strayed {} from the line (allowed {}).",
                        mm(max_dev),
                        mm(*max_dev_m)
                    ),
                ),
                None => (
                    false,
                    format!(
                        "Did not finish a lap: only {lap_text} in {:.0} s (limit {within_s} s); widest miss {}.",
                        last.t,
                        mm(max_dev)
                    ),
                ),
            };
            res("line", ok, msg)
        }
        Criterion::MeanSpeed {
            from_s,
            to_s,
            target_ms,
            tol_ms,
        } => {
            let v: Vec<f64> = t
                .samples
                .iter()
                .filter(|s| s.t >= *from_s && s.t <= *to_s)
                .map(|s| s.speed)
                .collect();
            if v.is_empty() {
                return res(
                    "speed",
                    false,
                    "The run was shorter than the speed window.".to_owned(),
                );
            }
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            let ok = (mean - target_ms).abs() <= *tol_ms;
            res(
                "mean speed",
                ok,
                format!(
                    "Mean speed from {from_s} s to {to_s} s was {mean:.3} m/s (wanted {target_ms} +- {tol_ms})."
                ),
            )
        }
        Criterion::Report {
            key,
            expected,
            tol,
            within_s,
        } => {
            let want = match expected {
                Expected::Value(v) => Some(*v),
                Expected::Measured(k) => t.measured.get(k).copied(),
            };
            let Some(want) = want else {
                return res(
                    "report",
                    false,
                    "The runner could not measure the true value.".to_owned(),
                );
            };
            let got = t
                .reports
                .iter()
                .rev()
                .find(|(tt, k, _)| k == key && within_s.is_none_or(|w| *tt <= w))
                .map(|(_, _, v)| *v);
            match got {
                None => res(
                    "report",
                    false,
                    format!(
                        "Nothing was reported under {key:?}{}.",
                        within_s.map_or(String::new(), |w| format!(" by {w} s"))
                    ),
                ),
                Some(v) => res(
                    "report",
                    (v - want).abs() <= *tol,
                    format!("Reported {v:.4}; the true value is {want:.4} (allowed +- {tol})."),
                ),
            }
        }
        Criterion::NoTip { max_tilt_rad } => {
            let worst = t.samples.iter().fold(0.0_f64, |a, s| a.max(s.tilt));
            res(
                "no tipping",
                worst <= *max_tilt_rad,
                format!(
                    "Tilted at most {} (allowed {}).",
                    deg(worst),
                    deg(*max_tilt_rad)
                ),
            )
        }
        Criterion::HeadingLock {
            distance_m,
            max_lateral_m,
            max_yaw_rad,
        } => {
            let along = last.pos[0] - first.pos[0];
            let lateral = (last.pos[1] - first.pos[1]).abs();
            let yaw = wrap(last.yaw - first.yaw).abs();
            let ok = (along - distance_m).abs() <= 0.03
                && lateral <= *max_lateral_m
                && yaw <= *max_yaw_rad;
            res(
                "heading lock",
                ok,
                format!(
                    "Drove {along:.3} m (wanted {distance_m} +- 0.030); {} off to the side (allowed {}); {} off in heading (allowed {}).",
                    mm(lateral),
                    mm(*max_lateral_m),
                    deg(yaw),
                    deg(*max_yaw_rad)
                ),
            )
        }
        Criterion::BelievedPose { tol_m } => match t.believed.last() {
            None => res(
                "believed pose",
                false,
                "The program never called axl_mark_pose.".to_owned(),
            ),
            Some((_, b)) => {
                let d = dist([b[0], b[1]], xy(last));
                res(
                    "believed pose",
                    d <= *tol_m,
                    format!(
                        "The believed pose ends {} from the true pose (allowed {}).",
                        mm(d),
                        mm(*tol_m)
                    ),
                )
            }
        },
        Criterion::EndNearStart { tol_m } => {
            let d = dist(xy(last), xy(first));
            res(
                "end near start",
                d <= *tol_m,
                format!("Ended {} from the start (allowed {}).", mm(d), mm(*tol_m)),
            )
        }
        Criterion::UsedPlot => res(
            "plot",
            t.plot_used,
            if t.plot_used {
                "The program used axl_plot.".to_owned()
            } else {
                "The program never called axl_plot.".to_owned()
            },
        ),
        Criterion::MaxParts { n } => res(
            "parts",
            t.part_count <= *n,
            format!("The robot has {} parts (allowed {n}).", t.part_count),
        ),
    }
}

/// Check every criterion of the mission (and `extra` ones) on one trace.
pub fn evaluate_seed(
    m: &MissionDef,
    extra: &[Criterion],
    ctx: &EvalContext,
    t: &RunTrace,
) -> SeedResult {
    let mut results: Vec<CriterionResult> = m
        .criteria
        .iter()
        .chain(extra)
        .map(|c| check(c, ctx, t))
        .collect();
    if let Some(e) = &t.program_error {
        results.insert(
            0,
            res("program", false, format!("The program stopped: {e}")),
        );
    }
    if t.unstable {
        results.insert(
            0,
            res(
                "simulation",
                false,
                "The simulation became unstable.".to_owned(),
            ),
        );
    }
    SeedResult {
        seed: t.seed,
        results,
    }
}

/// The result of grading a mission over its seeds.
#[derive(Clone, Debug, PartialEq)]
pub struct Grade {
    /// Every seed passes the mission's criteria.
    pub passed: bool,
    /// Stars earned (0 to 3).
    pub stars: u8,
    /// Per-seed verdicts of the mission's criteria.
    pub seeds: Vec<SeedResult>,
    /// Per-seed verdicts of the criteria of stars 2 and 3 (only filled when the mission passed).
    pub star_seeds: [Vec<SeedResult>; 2],
}

/// Grade `traces` (one per grading seed) against the mission. `hints_used`: a hint was opened.
pub fn grade(m: &MissionDef, ctx: &EvalContext, traces: &[RunTrace], hints_used: bool) -> Grade {
    let seeds: Vec<SeedResult> = traces
        .iter()
        .map(|t| evaluate_seed(m, &[], ctx, t))
        .collect();
    let passed = !seeds.is_empty() && seeds.iter().all(SeedResult::passed);
    let mut stars = u8::from(passed);
    let mut star_seeds: [Vec<SeedResult>; 2] = [Vec::new(), Vec::new()];
    if passed {
        let mut still = true;
        for (i, rule) in m.stars.iter().enumerate() {
            let rs: Vec<SeedResult> = traces
                .iter()
                .map(|t| evaluate_seed(m, &rule.criteria, ctx, t))
                .collect();
            let ok = rs.iter().all(SeedResult::passed) && !(rule.no_hints && hints_used);
            star_seeds[i] = rs;
            // Stars are earned in order.
            if ok && still {
                stars += 1;
            } else {
                still = false;
            }
        }
    }
    Grade {
        passed,
        stars,
        seeds,
        star_seeds,
    }
}

#[cfg(test)]
mod tests;
