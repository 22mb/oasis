# Contributing

Bug reports, documentation fixes and pull requests are welcome. For a larger
change, such as a new option or a change to the generated code's API, open an
issue first so the design can be agreed on before the work.

## Reporting a bug

Use the bug report form and include the smallest OpenAPI document that shows
the problem. Most bugs in a generator only reproduce with the document that
triggers them. The [playground](https://22mb.github.io/oasis/playground) is a
quick way to narrow one down.

Report security issues privately through
[GitHub's vulnerability reporting](https://github.com/22mb/oasis/security/advisories/new),
not in a public issue.

## Development

You need a stable Rust toolchain and Node.js 24 or later.

```sh
cargo test                         # unit tests and golden files
UPDATE_GOLDEN=1 cargo test         # regenerate tests/golden after an intended change
cd tests/ts && npm ci && npm test  # type-check and run the generated client against a test server
cd site && npm ci && npm run dev   # the documentation site
```

`tests/golden` holds the generated client for each fixture in
`tests/fixtures`. A change to the generator usually changes these files.
Review the diff and commit it with the change.

## Design

```
JSON/YAML files ──Documents──▶ serde_json::Value per file ──parse──▶ ir::Api ──emit──▶ TypeScript
```

`src/documents.rs` loads the entry document and every local file it references,
and resolves URI references (base URIs from `$self` and `$id`). `src/parse.rs`
handles everything about the OpenAPI document (`$ref` resolution, version
check, validation) and produces a language-neutral IR (`src/ir.rs`). Schemas
behind `$ref`s outside `components.schemas` are hoisted into named schemas
once, so references stay references and recursion is cheap. Emitters only
translate the IR. Adding a target language means adding one emitter next to
`src/typescript.rs`. The IR is public but not yet stable.

## Pull requests

- Keep a pull request to one change, and add a test or a fixture for it.
- Title it in [Conventional Commits](https://www.conventionalcommits.org/)
  form, such as `feat: support QUERY operations`. CI checks the title, which
  becomes the squashed commit and, for `feat`, `fix` and `perf`, the changelog
  entry.
- The generated code should stay dependency-free and type-check with
  TypeScript 5.0 or later. CI checks both.

## Releasing

1. Set `version` in `Cargo.toml`, and run `cargo check` and
   `cargo check --manifest-path playground/Cargo.toml` to update the
   lockfiles.
2. Draft the changelog section from the pull requests merged since the last
   release, then edit it for users: merge related entries and say what changes
   for them.

   ```sh
   npx git-cliff@2.14.2 --unreleased --tag v<version> --prepend CHANGELOG.md
   ```

3. Merge the release pull request, titled `release: <version>`, then tag its commit on `main` and push the
   tag, which runs the release workflow. CI checks that `Cargo.toml` and
   `CHANGELOG.md` match the title.

   ```sh
   git tag -a v<version> -m "oasis-gen <version>" && git push origin v<version>
   ```

By contributing, you agree that your contribution is licensed under the
project's dual MIT and Apache-2.0 license.
