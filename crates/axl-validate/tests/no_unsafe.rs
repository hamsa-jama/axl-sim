//! Spec rule 5: `unsafe` code lives only in `axl-mujoco`.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_axl_mujoco_may_contain_unsafe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut offenders = Vec::new();
    for top in ["crates", "xtask"] {
        let mut files = Vec::new();
        rust_files(&root.join(top), &mut files);
        for file in files {
            let rel = file.strip_prefix(&root).unwrap_or(&file).to_path_buf();
            if rel.starts_with("crates/axl-mujoco") || rel.ends_with("no_unsafe.rs") {
                continue;
            }
            let text = fs::read_to_string(&file).unwrap_or_default();
            // Mentions of the word in comments and the forbid attribute are fine.
            let has_unsafe = text
                .lines()
                .map(str::trim_start)
                .filter(|l| !l.starts_with("//") && !l.starts_with("#!["))
                .any(|l| {
                    l.contains("unsafe {") || l.contains("unsafe fn") || l.contains("unsafe impl")
                });
            if has_unsafe {
                offenders.push(rel);
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "unsafe code outside axl-mujoco: {offenders:?}"
    );
}

#[test]
fn every_other_library_crate_forbids_unsafe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for entry in fs::read_dir(&root).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "axl-mujoco" || name == "axl-app" {
            continue; // axl-app has no lib.rs; checked below
        }
        let lib = entry.path().join("src").join("lib.rs");
        let text = fs::read_to_string(&lib).unwrap_or_default();
        assert!(
            text.contains("#![forbid(unsafe_code)]"),
            "{name} must forbid unsafe_code"
        );
    }
    let main = fs::read_to_string(root.join("axl-app/src/main.rs")).unwrap_or_default();
    assert!(
        main.contains("#![forbid(unsafe_code)]"),
        "axl-app must forbid unsafe_code"
    );
}
