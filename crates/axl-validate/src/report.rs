//! Result types, the registry of tests and the report writers.

use std::fmt::Write as _;

/// One measured quantity compared with its expected value.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// What was measured.
    pub label: String,
    /// The textbook value, formatted for the report.
    pub expected: String,
    /// What the simulation gave, formatted for the report.
    pub measured: String,
    /// The allowed deviation, formatted for the report.
    pub tolerance: String,
    /// Whether the measurement is within tolerance.
    pub pass: bool,
}

impl Row {
    /// Pass when `|measured - expected| <= tol` (same unit as the values).
    pub fn abs(label: &str, expected: f64, measured: f64, tol: f64, unit: &str) -> Row {
        let diff = measured - expected;
        Row {
            label: label.to_owned(),
            expected: format!("{expected:.6} {unit}"),
            measured: format!("{measured:.6} {unit}"),
            tolerance: format!("±{tol:.6} {unit} (off by {diff:+.6})"),
            pass: diff.abs() <= tol && diff.is_finite(),
        }
    }

    /// Pass when the relative error is at most `rel` (0.01 = 1%).
    pub fn rel(label: &str, expected: f64, measured: f64, rel: f64, unit: &str) -> Row {
        let err = (measured - expected) / expected;
        Row {
            label: label.to_owned(),
            expected: format!("{expected:.6} {unit}"),
            measured: format!("{measured:.6} {unit}"),
            tolerance: format!("±{:.3}% (off by {:+.4}%)", rel * 100.0, err * 100.0),
            pass: err.abs() <= rel && err.is_finite(),
        }
    }

    /// Pass when `measured <= limit`.
    pub fn at_most(label: &str, limit: f64, measured: f64, unit: &str) -> Row {
        Row {
            label: label.to_owned(),
            expected: format!("≤ {limit:.6} {unit}"),
            measured: format!("{measured:.6} {unit}"),
            tolerance: "hard limit".to_owned(),
            pass: measured <= limit,
        }
    }

    /// Pass when `measured >= limit`.
    pub fn at_least(label: &str, limit: f64, measured: f64, unit: &str) -> Row {
        Row {
            label: label.to_owned(),
            expected: format!("≥ {limit:.6} {unit}"),
            measured: format!("{measured:.6} {unit}"),
            tolerance: "hard limit".to_owned(),
            pass: measured >= limit,
        }
    }

    /// A plain yes/no condition.
    pub fn check(label: &str, expected: &str, measured: &str, pass: bool) -> Row {
        Row {
            label: label.to_owned(),
            expected: expected.to_owned(),
            measured: measured.to_owned(),
            tolerance: "exact".to_owned(),
            pass,
        }
    }
}

/// The outcome of one validation test (`V01`, `V02`, ...).
#[derive(Clone, Debug, PartialEq)]
pub struct TestResult {
    /// Stable id, `V01` to `V18`.
    pub id: &'static str,
    /// One-line description.
    pub title: &'static str,
    /// The measurements.
    pub rows: Vec<Row>,
}

impl TestResult {
    /// True when there is at least one row and every row passed.
    pub fn passed(&self) -> bool {
        !self.rows.is_empty() && self.rows.iter().all(|r| r.pass)
    }
}

type TestFn = fn() -> TestResult;

/// Every test implemented so far, in id order.
const REGISTRY: &[(&str, TestFn)] = &[
    ("V01", crate::v_dynamics::v01_free_fall),
    ("V02", crate::v_dynamics::v02_pendulum_period),
    ("V03", crate::v_dynamics::v03_pendulum_energy),
    ("V04", crate::v_dynamics::v04_slope_friction),
    ("V05", crate::v_dynamics::v05_sliding_stop),
    ("V06", crate::v_dynamics::v06_rolling),
    ("V10", crate::v_robot::v10_gear_ratio),
    ("V12", crate::v_robot::v12_straight),
    ("V13", crate::v_robot::v13_turn_radius),
    ("V14", crate::v_robot::v14_rest_penetration),
    ("V15", crate::v_repro::v15_determinism),
];

/// Ids of the tests that exist.
pub fn available_ids() -> Vec<&'static str> {
    REGISTRY.iter().map(|(id, _)| *id).collect()
}

/// Run every implemented test.
pub fn run_all() -> Vec<TestResult> {
    REGISTRY.iter().map(|(_, f)| f()).collect()
}

/// Run only the tests whose ids are listed. Unknown ids are returned as `Err`.
pub fn run_selected(ids: &[String]) -> Result<Vec<TestResult>, String> {
    let mut out = Vec::new();
    for id in ids {
        let (_, f) = REGISTRY
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(id))
            .ok_or_else(|| format!("no validation test called {id}"))?;
        out.push(f());
    }
    Ok(out)
}

/// The report as Markdown.
pub fn to_markdown(results: &[TestResult]) -> String {
    let passed = results.iter().filter(|r| r.passed()).count();
    let mut s = String::new();
    let _ = writeln!(s, "# AXL accuracy report\n");
    let _ = writeln!(
        s,
        "Physics verified: {passed} of {} implemented tests pass. Hardware calibration is separate (see `docs/calibration.md`).\n",
        results.len()
    );
    let _ = writeln!(s, "| Test | Result | Description |\n| --- | --- | --- |");
    for r in results {
        let _ = writeln!(
            s,
            "| {} | {} | {} |",
            r.id,
            if r.passed() { "pass" } else { "FAIL" },
            r.title
        );
    }
    for r in results {
        let _ = writeln!(s, "\n## {} {}\n", r.id, r.title);
        let _ = writeln!(
            s,
            "| Check | Expected | Measured | Tolerance | Result |\n| --- | --- | --- | --- | --- |"
        );
        for row in &r.rows {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} |",
                row.label,
                row.expected,
                row.measured,
                row.tolerance,
                if row.pass { "pass" } else { "FAIL" }
            );
        }
    }
    s
}

/// The report as JSON (written by hand: no JSON crate is on the approved list).
pub fn to_json(results: &[TestResult]) -> String {
    let mut s = String::from("{\n  \"tests\": [");
    for (i, r) in results.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "\n    {{\"id\": {}, \"title\": {}, \"passed\": {}, \"rows\": [",
            json_str(r.id),
            json_str(r.title),
            r.passed()
        );
        for (j, row) in r.rows.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "\n      {{\"label\": {}, \"expected\": {}, \"measured\": {}, \"tolerance\": {}, \"pass\": {}}}",
                json_str(&row.label),
                json_str(&row.expected),
                json_str(&row.measured),
                json_str(&row.tolerance),
                row.pass
            );
        }
        s.push_str("\n    ]}");
    }
    let _ = write!(
        s,
        "\n  ],\n  \"all_passed\": {}\n}}\n",
        results.iter().all(|r| r.passed())
    );
    s
}

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
    fn row_helpers_judge_correctly() {
        assert!(Row::abs("a", 1.0, 1.001, 0.002, "m").pass);
        assert!(!Row::abs("a", 1.0, 1.01, 0.002, "m").pass);
        assert!(!Row::abs("a", 1.0, f64::NAN, 0.002, "m").pass);
        assert!(Row::rel("r", 100.0, 100.4, 0.005, "x").pass);
        assert!(!Row::rel("r", 100.0, 101.0, 0.005, "x").pass);
        assert!(Row::at_most("m", 1.0, 0.5, "mm").pass);
        assert!(!Row::at_most("m", 1.0, 1.5, "mm").pass);
        assert!(Row::at_least("m", 1.0, 1.5, "mm").pass);
        assert!(!Row::at_least("m", 1.0, 0.5, "mm").pass);
        assert!(Row::check("c", "x", "x", true).pass);
    }

    #[test]
    fn empty_results_do_not_pass() {
        let t = TestResult {
            id: "V00",
            title: "t",
            rows: vec![],
        };
        assert!(!t.passed());
    }

    #[test]
    fn json_escapes_special_characters() {
        assert_eq!(json_str("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(json_str("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn reports_mention_every_test() {
        let t = TestResult {
            id: "V99",
            title: "demo",
            rows: vec![Row::abs("x", 1.0, 1.0, 0.1, "m")],
        };
        let md = to_markdown(std::slice::from_ref(&t));
        assert!(md.contains("V99") && md.contains("pass"));
        let js = to_json(std::slice::from_ref(&t));
        assert!(js.contains("\"id\": \"V99\"") && js.contains("\"all_passed\": true"));
    }

    #[test]
    fn unknown_ids_are_reported() {
        assert!(run_selected(&["V99".to_owned()]).is_err());
        assert!(available_ids().contains(&"V01"));
    }
}
