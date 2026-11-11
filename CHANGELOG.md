# Changelog

All notable changes to oasis-gen are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Until 1.0.0, a minor version may
change the generated code or the CLI in incompatible ways.

## [0.9.0](https://github.com/22mb/oasis/compare/v0.8.0...v0.9.0) - 2026-11-11

### Fixed

- **Breaking:** Leave 304 out of the statuses that default and no key cover ([#102](https://github.com/22mb/oasis/pull/102))

## [0.8.0](https://github.com/22mb/oasis/compare/v0.7.0...v0.8.0) - 2026-11-07

### Fixed

- Add a query API key to a GET request in Firefox ([#93](https://github.com/22mb/oasis/pull/93))
- Leave out a flagged property from a nested allOf of references ([#95](https://github.com/22mb/oasis/pull/95))
- **Breaking:** A `text/*` media type declared binary (a schema without a
  `type`) in one response was read as a `Blob` in every response of the
  operation, even where its type was `string` or an event stream. It is now
  read as a `Blob` only for the statuses of the response that declares it,
  so `data` is what its type says. Code that read such `data` as a `Blob`
  against its type must read it as typed.
  ([#97](https://github.com/22mb/oasis/pull/97))
- **Breaking:** The `data` of a 204, 205 or 304 response that the document
  covers only with `2XX`, `3XX` or `default` was typed as that key's body,
  though the client returns `undefined` for it. It is now `undefined`. With
  `200` and `default: Error`, `data` in `if (!res.ok)` now includes the
  `undefined` of a 304: read it with `?.`, or narrow by `status` first.
  ([#96](https://github.com/22mb/oasis/pull/96))
- Send a request resent through auth with only the credentials chosen for it ([#98](https://github.com/22mb/oasis/pull/98))
- **Breaking:** A response declared as the range `text/*` was typed
  `string`, though the client reads `text/event-stream` and `+json` types
  under it as an event stream or as JSON. Its `data` is now `unknown`, as
  for `*/*` and `application/*`. Check its type before use, or declare the
  media types the response sends, such as `text/plain`, which stay `string`.
  ([#99](https://github.com/22mb/oasis/pull/99))
- **Breaking:** A subtype that adds nothing to a parent that only lists its
  subtypes in `oneOf` or `anyOf` was typed `unknown`. Such a document is now
  an error. Give the subtype or its parent a `type` or `properties`, such as
  the discriminator property.
  ([#100](https://github.com/22mb/oasis/pull/100))

## [0.7.0](https://github.com/22mb/oasis/compare/v0.6.0...v0.7.0) - 2026-11-07

### Fixed

- **Breaking:** `minItems` was not read, so an array with `minItems: 1` took
  an empty array. An array without `prefixItems` is now a tuple whose first
  `minItems` items are required: `[T, ...T[]]` for 1, `[T, T, ...T[]]` for 2.
  Its first items are also no longer `T | undefined` under
  `noUncheckedIndexedAccess`.
  ([#77](https://github.com/22mb/oasis/pull/77))
- **Breaking:** In a `multipart/form-data` body composed with `allOf` or
  `oneOf`, a part without a `type` was `unknown`. It is now a `Blob`, as in a
  body whose schema is an object, so a value other than a `Blob` no longer
  type-checks there.
  ([#78](https://github.com/22mb/oasis/pull/78))
- Send encoded binary parts of a composed multipart body as octets ([#79](https://github.com/22mb/oasis/pull/79))
- Leave out what a subtype flags from the parent it extends ([#80](https://github.com/22mb/oasis/pull/80))
- **Breaking:** A component name with other characters than ASCII letters,
  digits, `.`, `-` and `_`, which the specification does not allow, was
  exported without them (`ユーザー` as `_`, `Café` as `Caf`). Such a name is now
  an error. Rename the component.
  ([#88](https://github.com/22mb/oasis/pull/88))
- **Breaking:** `propertyNames` was not read, so a map whose keys an `enum`
  limits took any key. Where `propertyNames` lists the names with `enum` or
  `const`, the object now has an optional property for each of them, of the
  type of `additionalProperties`, and no other key.
  ([#89](https://github.com/22mb/oasis/pull/89))

## [0.6.0](https://github.com/22mb/oasis/compare/v0.5.1...v0.6.0) - 2026-11-06

### Fixed

- **Breaking:** A `text/*` response whose schema has no `type` (binary data;
  `type: string, format: binary` in 3.0) was read as UTF-8 text, so a CSV in
  Shift_JIS, for example, lost its bytes. It is now typed and read as a `Blob`.
  Use `await data.text()` where text was wanted.
  ([#69](https://github.com/22mb/oasis/pull/69))
- Leave a flagged property out of an allOf member's $ref ([#70](https://github.com/22mb/oasis/pull/70))
- Treat an empty server-sent event type as the default ([#71](https://github.com/22mb/oasis/pull/71))
- Read a flow-style YAML document that is not JSON ([#72](https://github.com/22mb/oasis/pull/72))
- **Breaking:** A request body declared by a media type range such as
  `image/*` could leave out `contentType`, and was then sent as another of the
  operation's media types. `contentType` is now required and typed by the range
  (`` `image/${string}` ``). For `*/*` it stays a `string`, required only when
  the operation also has `application/json`.
  ([#73](https://github.com/22mb/oasis/pull/73))
- **Breaking:** A request body could be a value that fetch sends as another
  media type. A multipart body other than `multipart/form-data` is now a `Blob`
  instead of `FormData`: build it yourself and give the `Blob` a `type` with
  the boundary, such as `multipart/mixed; boundary=b`. A binary body no longer
  takes `FormData` or `URLSearchParams`.
  ([#74](https://github.com/22mb/oasis/pull/74))
- Leave out a property that another allOf $ref member flags ([#75](https://github.com/22mb/oasis/pull/75))

## [0.5.1](https://github.com/22mb/oasis/compare/v0.5.0...v0.5.1) - 2026-11-03

### Fixed

- Type discriminator tags by the discriminating property ([#54](https://github.com/22mb/oasis/pull/54))
- Stop generation hanging on a cycle of references ([#55](https://github.com/22mb/oasis/pull/55))
- Report syntax errors as file:line:column ([#64](https://github.com/22mb/oasis/pull/64))
- Report where in the document each error is ([#65](https://github.com/22mb/oasis/pull/65))
- Give every error message the same form ([#66](https://github.com/22mb/oasis/pull/66))

## [0.5.0](https://github.com/22mb/oasis/compare/v0.4.1...v0.5.0) - 2026-11-02

### Fixed

- **Breaking:** `type: object` next to `allOf`, `oneOf` or `$ref` added an index
  signature, so misspelled property names compiled in requests and undeclared
  properties could be read from responses. It now becomes `object` in the
  intersection, so both are type errors.
  ([#49](https://github.com/22mb/oasis/pull/49))

## [0.4.1](https://github.com/22mb/oasis/compare/v0.4.0...v0.4.1) - 2026-10-28

### Fixed

- A generated file declared status aliases it did not use, such as
  `Status1xx`, so a project that type-checks it with `noUnusedLocals` failed
  since 0.4.0. It now declares only the ones it uses.
  ([#43](https://github.com/22mb/oasis/pull/43))

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
