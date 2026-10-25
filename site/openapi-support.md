# OpenAPI support

This page lists what oasis-gen accepts from a document and what it does with
each feature. The support levels mean the following:

- **Supported**: the feature affects the generated client.
- **Ignored**: the feature is accepted but does not affect the generated client.
- **Rejected**: the feature stops generation with an error.
- **Not supported**: the feature is read as noted, or not read at all.

## Versions

| Version | Support |
| --- | --- |
| OpenAPI 3.2 | Supported |
| OpenAPI 3.1 | Supported; read exactly like 3.2 |
| OpenAPI 3.0 | Supported, with the differences below |
| Swagger 2.0 (`swagger: "2.0"`) | Not supported |

In an OpenAPI 3.0 document, these keywords are handled as follows:

| Keyword | Handling |
| --- | --- |
| `nullable: true` | Adds `null` to the type. On an `enum`, `null` is added even if the list omits it. Other generators read it this way, although 3.0.3 itself leaves such an `enum` excluding `null` |
| `nullable: true` next to a `$ref`, or on an `allOf` with one `$ref` | Adds `null` to the referenced type |
| `example`, a boolean `exclusiveMinimum`, `format: binary` or `byte` | Ignored |

`nullable` in a 3.1 or 3.2 document is rejected, because the specification
would have it silently ignored in those versions. Use `type: [..., "null"]`
instead.

## Documents

| Feature | Support | Notes |
| --- | --- | --- |
| JSON and YAML | Supported | From a file, or from stdin with `-` |
| `$ref`s to other local files | Supported | Whole documents or fragments, resolved relative to the referencing document |
| `$self`, and `$id` and `$anchor` in schemas | Supported | A `$self` URI such as `https://api.example.com/openapi/root.yaml` is mapped onto the file's own directory, so `schemas/pet.yaml` next to it is found locally |
| `components.mediaTypes` and `$ref`s to them | Supported | |
| Remote `$ref`s, and URLs as input | Not supported | `https://...` URIs that do not map onto a local file |

Schemas from other files appear in `components["refs"]` (see
[Schemas](./schemas#type-aliases-and-components)).

## Schemas

See [Schemas](./schemas) for the TypeScript type that each keyword becomes.

| Feature | Support | Notes |
| --- | --- | --- |
| JSON Schema 2020-12 | Supported | `type` arrays, `const`, `enum`, `allOf`, `oneOf`, `anyOf`, `prefixItems`, `additionalProperties`, `$ref` with sibling keywords, and `readOnly` and `writeOnly` (see [Strict types](./strict-types#requests-and-responses-have-separate-types)) |
| Recursive schemas | Supported | Through properties, arrays, and `additionalProperties`, including `$ref`s into `$defs` and other locations outside `components.schemas` |
| `discriminator` on `oneOf` or `anyOf` | Supported | Explicit and implicit mappings, and `defaultMapping` |
| Subtypes listed in the parent's `oneOf`, each extending it with `allOf` | Supported | See [Subtypes](./schemas#subtypes) |
| Other cycles through `$ref`, `allOf`, `oneOf` or `anyOf` alone | Rejected | Cycles with no object or array in between |
| `discriminator` on an `allOf` parent | Not supported | The parent is not turned into a union of its children |
| `$dynamicRef` | Not supported | Read as `unknown` |

## Operations

| Feature | Support | Notes |
| --- | --- | --- |
| `get`, `put`, `post`, `delete`, `options`, `head`, `patch`, `trace` | Supported | |
| `query` (the HTTP `QUERY` method) | Supported | |
| `additionalOperations` | Supported | Custom methods such as `LINK` |
| Some `additionalOperations` names | Rejected | `delete`, `get`, `head`, `options`, `post`, and `put` in any case, and `PATCH`, `TRACE`, and `QUERY` in uppercase, because fetch would send them as the fixed fields' methods; also `with` and `then`, which the client reserves |
| Callbacks and webhooks | Not supported | |

## Parameters

| Location | Styles | Notes |
| --- | --- | --- |
| `query` | `form`, `deepObject` | `allowReserved` is supported. `spaceDelimited` and `pipeDelimited` are rejected |
| `path` | `simple` | `label` and `matrix` are rejected |
| `header` | `simple` | |
| `cookie` | `form` (percent-encoded), `cookie` (sent as-is) | Exploded values are joined with `; ` for both styles, because the specification rules out `&` for cookies |
| `querystring` | Supported | Typed and encoded by its media type; see [`querystring` parameters](./client#querystring-parameters) |

Every style supports `explode`. Parameters with `content` instead of a schema
are supported. A JSON query parameter is typed from its schema and serialized
as JSON. Other parameters with `content` are passed as strings that are already
serialized.

Cookie parameters and cookie API keys are sent as a `Cookie` header, which
browsers ignore in `fetch`. They work in Node, Deno, and Bun.

## Request and response bodies

See [Request bodies](./client#request-bodies) and
[Response bodies](./client#response-bodies) for how each media type is typed
and decoded.

| Feature | Support | Notes |
| --- | --- | --- |
| Sequential media types and `itemSchema` | Supported | JSON Lines, NDJSON, JSON text sequences, and server-sent events, streamed as async iterables |
| Multipart request bodies | Supported | Passed as `FormData`; part schemas and the Encoding Object are not used |
| `multipart/mixed` streaming | Not supported | |
| Typed response headers | Not supported | |

## Servers and security

| Feature | Support | Notes |
| --- | --- | --- |
| `servers` at the top level, on path items, and on operations | Supported | Variables are set to their defaults; see [Servers and `baseUrl`](./client#servers-and-baseurl) |
| Security schemes | Supported | `apiKey`, `http`, `oauth2`, `openIdConnect`, and `mutualTLS`, through [`auth`](./authentication) |
| OAuth2 flows | Not supported | You obtain tokens yourself |
