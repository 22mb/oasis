//! Compares generated output for each fixture with its golden file.
//! A fixture is a document in `tests/fixtures`, or a directory there whose
//! entry document is `openapi.yaml`. Run with `UPDATE_GOLDEN=1` to rewrite
//! the golden files.

use std::path::Path;

#[test]
fn typescript_golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    for entry in std::fs::read_dir(root.join("fixtures")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_owned();
        let spec = if path.is_dir() {
            path.join("openapi.yaml")
        } else {
            path
        };
        let actual = oasis::generate_file(&spec, oasis::Lang::TypeScript)
            .unwrap_or_else(|e| panic!("{}: {e}", spec.display()));
        let golden = root.join("golden").join(name).with_extension("ts");
        if update {
            std::fs::write(&golden, &actual).unwrap();
        } else {
            let expected = std::fs::read_to_string(&golden).unwrap_or_else(|_| {
                panic!("missing {}; run with UPDATE_GOLDEN=1", golden.display())
            });
            assert!(
                actual == expected,
                "{} differs from {}; run with UPDATE_GOLDEN=1 and review the diff",
                spec.display(),
                golden.display()
            );
        }
    }
}
