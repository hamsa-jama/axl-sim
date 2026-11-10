//! Headless mode: no window, no GPU. `axl --headless --version` prints the version;
//! `axl --headless --mission m03 --controller robot.wasm --seed 1` runs the mission flat out and
//! prints a JSON result. The exit code is 0 when the mission passed and 1 when it did not (2 when
//! the run could not be set up).

use std::fmt::Write as _;
use std::process::ExitCode;

use axl_missions::eval::Grade;
use axl_missions::library::mission;
use axl_parts::catalog::Catalog;
use axl_sim::mission::{GradedRun, grade_mission, robot_assembly};

use crate::cli::Args;

/// Exit code for a run that could not be set up.
const EXIT_SETUP: u8 = 2;

/// Run in headless mode and return the process exit code.
pub fn run(args: &Args) -> ExitCode {
    let Some(id) = &args.mission else {
        println!("{}", version_line());
        return ExitCode::SUCCESS;
    };
    match grade_from_args(args, id) {
        Ok((json, passed)) => {
            println!("{json}");
            if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(message) => {
            eprintln!("axl: {message}");
            println!("{{\"error\": {}}}", json_str(&message));
            ExitCode::from(EXIT_SETUP)
        }
    }
}

fn grade_from_args(args: &Args, id: &str) -> Result<(String, bool), String> {
    let m = mission(id).map_err(|e| e.to_string())?;
    let wasm = match &args.controller {
        Some(p) => Some(std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?),
        None => None,
    };
    let cat = Catalog::builtin();
    let asm = robot_assembly(&m, &cat, args.robot.as_deref()).map_err(|e| e.to_string())?;
    let seeds: Vec<u64> = if args.all_seeds {
        m.grading_seeds.clone()
    } else {
        vec![args.seed]
    };
    let run =
        grade_mission(&m, &asm, &cat, wasm.as_deref(), &seeds, false).map_err(|e| e.to_string())?;
    Ok((result_json(id, &run), run.grade.passed))
}

/// The one-line version string, `axl <version>`.
pub fn version_line() -> String {
    format!("axl {}", env!("CARGO_PKG_VERSION"))
}

/// The JSON printed for a graded mission (written by hand: no JSON crate is on the approved list).
pub fn result_json(id: &str, run: &GradedRun) -> String {
    let g: &Grade = &run.grade;
    let mut s = String::new();
    let _ = write!(
        s,
        "{{\"mission\": {}, \"passed\": {}, \"stars\": {}, \"seeds\": [",
        json_str(id),
        g.passed,
        g.stars
    );
    for (i, (r, run)) in g.seeds.iter().zip(&run.runs).enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let t = &run.trace;
        let _ = write!(
            s,
            "{{\"seed\": {}, \"passed\": {}, \"time_s\": {:.2}, \"energy_wh\": {:.4}, \"collisions\": {}, \"criteria\": [",
            r.seed,
            r.passed(),
            t.last().map_or(0.0, |x| x.t),
            t.energy_wh,
            t.collisions
        );
        for (j, c) in r.results.iter().enumerate() {
            if j > 0 {
                s.push_str(", ");
            }
            let _ = write!(
                s,
                "{{\"name\": {}, \"ok\": {}, \"message\": {}}}",
                json_str(&c.name),
                c.ok,
                json_str(&c.message)
            );
        }
        s.push_str("], \"measured\": {");
        for (j, (k, v)) in t.measured.iter().enumerate() {
            if j > 0 {
                s.push_str(", ");
            }
            let _ = write!(s, "{}: {v:.4}", json_str(k));
        }
        s.push_str("}, \"log\": [");
        for (j, line) in run.logs.iter().take(MAX_LOG_LINES).enumerate() {
            if j > 0 {
                s.push_str(", ");
            }
            s.push_str(&json_str(line));
        }
        s.push_str("]}");
    }
    s.push_str("]}");
    s
}

/// Lines of program output kept per seed in the JSON.
const MAX_LOG_LINES: usize = 200;

fn json_str(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_has_package_version() {
        assert_eq!(version_line(), format!("axl {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_str("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }
}
