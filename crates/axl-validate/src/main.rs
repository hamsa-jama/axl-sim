//! `axl-validate`: run the accuracy suite and write the report.
//!
//! Usage: `axl-validate [--out DIR] [V01 V02 ...]`. Writes `report.md` and
//! `report.json` to DIR (default `target/validation`) and exits 1 if any test
//! fails.

use std::path::PathBuf;
use std::process::ExitCode;

use axl_validate::report::{available_ids, to_json, to_markdown};
use axl_validate::{run_all, run_selected};

fn main() -> ExitCode {
    let mut out = PathBuf::from("target/validation");
    let mut ids = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => match args.next() {
                Some(dir) => out = PathBuf::from(dir),
                None => return usage("--out needs a folder"),
            },
            "--list" => {
                println!("{}", available_ids().join(" "));
                return ExitCode::SUCCESS;
            }
            id if !id.starts_with('-') => ids.push(id.to_owned()),
            other => return usage(&format!("unknown option {other}")),
        }
    }

    let results = if ids.is_empty() {
        run_all()
    } else {
        match run_selected(&ids) {
            Ok(r) => r,
            Err(e) => return usage(&e),
        }
    };

    for r in &results {
        println!(
            "{} {:<4} {}",
            r.id,
            if r.passed() { "pass" } else { "FAIL" },
            r.title
        );
        for row in r.rows.iter().filter(|row| !row.pass) {
            println!(
                "     {}: expected {}, measured {} (tolerance {})",
                row.label, row.expected, row.measured, row.tolerance
            );
        }
    }
    if let Err(e) = write_reports(&out, &results) {
        eprintln!(
            "axl-validate: could not write the report to {}: {e}",
            out.display()
        );
        return ExitCode::FAILURE;
    }
    println!("report written to {}", out.display());
    if results.iter().all(|r| r.passed()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn write_reports(
    dir: &std::path::Path,
    results: &[axl_validate::TestResult],
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("report.md"), to_markdown(results))?;
    std::fs::write(dir.join("report.json"), to_json(results))
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("axl-validate: {msg}\nusage: axl-validate [--out DIR] [--list] [V01 V02 ...]");
    ExitCode::from(2)
}
