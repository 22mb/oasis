# Changelog

All notable changes to oasis-gen are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Until 1.0.0, a minor version may
change the generated code or the CLI in incompatible ways.

## [0.4.1](https://github.com/22mb/oasis/compare/v0.4.0...v0.4.1) - 2026-10-28

### Fixed

- Emit only the status aliases a generated file uses ([#43](https://github.com/22mb/oasis/pull/43))

## [0.4.0](https://github.com/22mb/oasis/compare/v0.3.0...v0.4.0) - 2026-10-27

### Changed

- **Breaking:** The generator computes which statuses each response key
  covers, instead of conditional types at every call, so calls type-check
  faster. `ApiResponse` takes the operation instead of its `responses`:
  `ApiResponse<paths["/pets/{petId}"]["GET"]>`. Each operation in `paths` has
  a `statuses` member. ([#39](https://github.com/22mb/oasis/pull/39))

### Fixed

- **Breaking:** A multipart body was sent with the `Content-Type` set in the
  client's or the call's `headers`, without a boundary, so the server could not
  parse it. `fetch` now sets the header, boundary included. Header objects can
  no longer name `Content-Type`, since the body's media type sets it; to send a
  body for a range such as `*/*` as a given media type, pass `contentType`.
  ([#41](https://github.com/22mb/oasis/pull/41))

## [0.3.0](https://github.com/22mb/oasis/compare/v0.2.0...v0.3.0) - 2026-10-25

### Added

- **Breaking:** Type multipart/form-data bodies by their schema and follow Encoding Objects ([#36](https://github.com/22mb/oasis/pull/36))

## [0.2.0](https://github.com/22mb/oasis/compare/v0.1.2...v0.2.0) - 2026-10-25

### Removed

- **Breaking:** The generated `components` no longer has `refs` and
  `requestRefs`. Schemas in other files or under `$defs` are still typed, and
  are reached through the component schemas and operations that refer to
  them. Named exports (`import type { Pet }`), `schemas` and
  `components["schemas"]` are unchanged.

### Added

- Node.js 22 is supported. The npm package now requires Node.js 22 or later
  instead of 24.

### Fixed

- `patternProperties` is no longer ignored. With `additionalProperties: false`,
  a value with a key that matches a pattern was rejected. TypeScript cannot
  restrict keys to a pattern, so such an object now has an index signature
  over every key, typed as the patterns and `additionalProperties` allow.
- The variant that a discriminator's `defaultMapping` names no longer requires
  the discriminating property. The specification lets a value leave it out,
  and uses the `defaultMapping` schema then.
- `auth()` accepts `undefined` credentials, such as an unset environment
  variable, under `exactOptionalPropertyTypes`. It already treated them as
  unavailable at runtime.
- A call can leave out an optional request body without naming a media type,
  also when the operation declares several and none is implied.
- A `Cookie` header in a call's `headers` option now joins the `cookie`
  parameters and replaces only the cookies of the same name. Before, it
  replaced them all.

### Security

- The generated client rejects a path that starts with `//` with a
  `TypeError`. With no server URL and no `baseUrl`, browsers resolved such a
  path as a URL on another host, and `auth()` sent credentials there.

## [0.1.2](https://github.com/22mb/oasis/compare/v0.1.1...v0.1.2) - 2026-10-25

### Changed

- The documentation no longer lists Deno as a supported runtime for the
  generated client, since CI tests the client only in Node.js and Bun.

## [0.1.1](https://github.com/22mb/oasis/releases/tag/v0.1.1) - 2026-10-25

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
