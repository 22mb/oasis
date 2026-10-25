# Changelog

All notable changes to oasis-gen are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Until 1.0.0, a minor version may
change the generated code or the CLI in incompatible ways.

## [Unreleased]

### Security

- The generated client rejects a path that starts with `//` with a
  `TypeError`. With no server URL and no `baseUrl`, browsers resolved such a
  path as a URL on another host, and `auth()` sent credentials there.

## [0.1.2] - 2026-10-25

### Changed

- The documentation no longer lists Deno as a supported runtime for the
  generated client, since CI tests the client only in Node.js and Bun.

## [0.1.1] - 2026-10-25

The first release. 0.1.0 was tagged, but its release stopped before the
`oasis-gen` package was published to npm.

### Added

- A CLI, `oasis-gen <SPEC> [-o <FILE>]`, that reads an OpenAPI 3.0, 3.1 or
  3.2 document in JSON or YAML, including `$ref`s to other local files, and
  writes one dependency-free TypeScript file.
- A typed client, `createClient`, with calls keyed by HTTP method and path
  template. `status` narrows `data` to the declared response, and undeclared
  statuses stay in the type.
- Request bodies sent as the operation's media type, and response bodies
  decoded from `Content-Type`, including JSON Lines, NDJSON, JSON text
  sequences and server-sent events as async iterables. A body that does not
  match its `Content-Type` throws a `DecodeError`.
- Middleware for the client and for a single call, and an `auth` middleware
  that applies each operation's `security` requirements.
- An export for each component schema, the values of each enum, and a request
  variant for each schema with `readOnly` or `writeOnly` properties.
- OpenAPI 3.2 features: `QUERY` operations, `additionalOperations`,
  `in: querystring` parameters, `itemSchema`, `components.mediaTypes`, and
  `$self`.
- Prebuilt binaries for macOS, Linux and Windows on arm64 and x64, published
  to npm as `oasis-gen`.

[Unreleased]: https://github.com/22mb/oasis/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/22mb/oasis/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/22mb/oasis/releases/tag/v0.1.1
