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

### Principles for the generated types

- The types express what the OpenAPI specification allows, following both its
  text and its intent. They are neither narrower (rejecting values the
  document allows) nor wider (accepting what the specification says not to
  send). For example, `readOnly` properties are typed `?: never` in requests.
- Where TypeScript cannot express a rule exactly, the generated type is the
  closest one that still accepts every allowed value. For example,
  `patternProperties` becomes an index signature that covers every key.
- Strictness is a consequence of the first principle, not a goal in itself.
  For example, status codes the document does not declare remain in the
  response type, because the specification says they can occur.
- A document that does not conform to the specification is rejected with an
  error. There is no lenient mode.
- Inputs the specification does not define, such as the credentials passed to
  `auth()`, are handled as documented for the runtime.

## Pull requests

- Keep a pull request to one change, and add a test or a fixture for it.
- Title it in [Conventional Commits](https://www.conventionalcommits.org/)
  form, such as `feat: support QUERY operations`. CI checks the title, which
  becomes the squashed commit and, for `feat`, `fix` and `perf`, the changelog
  entry.
- The generated code should stay dependency-free and type-check with
  TypeScript 5.0 or later. CI checks both.

## Releasing

1. Run the **Prepare release** workflow from the Actions tab. It picks the
   next version from the pull requests merged since the last release (until
   1.0.0, a breaking change bumps the minor version and anything else the
   patch version), sets it in `Cargo.toml` and both lockfiles, adds its section
   to `CHANGELOG.md` from their titles, and pushes the branch
   `release/<version>`.
2. Open the pull request from the link in the run's summary. Its title is
   `release: <version>`, and CI checks that `Cargo.toml` and `CHANGELOG.md`
   match it. Edit the changelog on the branch if an entry needs more than its
   title, such as how to migrate from a breaking change.
3. Merge it. The release workflow tags the commit `v<version>`, builds every
   platform, and publishes the npm packages and the GitHub release.

By contributing, you agree that your contribution is licensed under the
project's dual MIT and Apache-2.0 license.
