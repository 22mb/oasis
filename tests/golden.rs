//! Compares generated output for each fixture with its golden file.
//! A fixture is a document in `tests/fixtures`, or a directory there whose
//! entry document is `openapi.yaml`. Run with `UPDATE_GOLDEN=1` to rewrite
//! the golden files.

use std::path::Path;

#[test]
fn typescript_golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("fixtures"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir() || p.extension().is_some_and(|e| e == "yaml" || e == "json"))
        .collect();
    fixtures.sort();
    assert!(!fixtures.is_empty());
    let mut failures = Vec::new();
    for path in fixtures {
        let name = path.file_stem().unwrap().to_owned();
        let spec = if path.is_dir() {
            path.join("openapi.yaml")
        } else {
            path
        };
        let actual = oasis_gen::generate_file(&spec, oasis_gen::Lang::TypeScript)
            .unwrap_or_else(|e| panic!("{}: {e}", spec.display()));
        let golden = root.join("golden").join(name).with_extension("ts");
        // As in the repository, for the messages below.
        let shown = golden.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap();
        if update {
            std::fs::write(&golden, &actual).unwrap();
            continue;
        }
        let Ok(expected) = std::fs::read_to_string(&golden) else {
            failures.push(format!("{} is missing", shown.display()));
            continue;
        };
        if actual != expected {
            failures.push(format!(
                "{}\n{}",
                shown.display(),
                first_difference(&expected, &actual)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} golden file(s) differ; run with UPDATE_GOLDEN=1 and review the diff\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// The first line that differs, with its number. A golden file has thousands
/// of lines, so the rest is left to `git diff` after an update.
fn first_difference(expected: &str, actual: &str) -> String {
    let (mut e, mut a) = (expected.lines(), actual.lines());
    let mut line = 1;
    loop {
        match (e.next(), a.next()) {
            (Some(e), Some(a)) if e == a => line += 1,
            (None, None) => return "  only line endings differ".into(),
            (e, a) => {
                let show = |l: Option<&str>| l.map_or("(end of file)".into(), |l| format!("{l:?}"));
                return format!(
                    "  line {line}\n  expected: {}\n  actual:   {}",
                    show(e),
                    show(a)
                );
            }
        }
    }
}
