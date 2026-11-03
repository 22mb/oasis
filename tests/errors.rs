//! Compares what the CLI prints for each invalid document with its expected
//! output. A case is a document in `tests/errors`, or a directory there whose
//! entry document is `openapi.yaml`; the expected standard error is in
//! `<case>.stderr` next to it. Run with `UPDATE_GOLDEN=1` to rewrite them.

use std::path::Path;
use std::process::Command;

#[test]
fn error_messages() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/errors");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut cases: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir() || p.extension().is_some_and(|e| e == "yaml" || e == "json"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty());
    // Files read through a `$ref` are named by their full path. On Windows it
    // may carry the `\\?\` prefix of a canonical path.
    let canonical = root.canonicalize().unwrap().display().to_string();
    let dirs = [
        canonical.clone(),
        canonical.trim_start_matches(r"\\?\").to_string(),
        root.display().to_string(),
    ];
    let mut failures = Vec::new();
    for path in cases {
        let name = path.file_name().unwrap().to_str().unwrap();
        // Relative to `tests/errors`, as a user would type it.
        let spec = if path.is_dir() {
            format!("{name}/openapi.yaml")
        } else {
            name.to_string()
        };
        let out = Command::new(env!("CARGO_BIN_EXE_oasis-gen"))
            .arg(&spec)
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{spec}");
        assert!(out.stdout.is_empty(), "{spec}");
        let mut actual = String::from_utf8(out.stderr).unwrap();
        for dir in &dirs {
            actual = actual.replace(dir.as_str(), ".");
        }
        // Windows prints paths with `\`.
        let actual = actual.replace('\\', "/");
        let expected_path = root.join(name).with_extension("stderr");
        if update {
            std::fs::write(&expected_path, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&expected_path).unwrap_or_default();
        if actual != expected {
            failures.push(format!(
                "{spec}\n  expected: {expected:?}\n  actual:   {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) differ; run with UPDATE_GOLDEN=1 and review the diff\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
