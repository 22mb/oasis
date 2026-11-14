# OpenAPI support

This page lists what oasis-gen accepts from a document and what it does with
each feature. The support levels mean the following:

- **Supported**: the feature affects the generated client.
- **Ignored**: the feature is accepted but does not affect the generated client.
- **Rejected**: the feature stops generation with an error.
- **Not supported**: the feature is handled only as noted, or not at all.

## Versions

| Version | Support |
| --- | --- |
| OpenAPI 3.2 | Supported |
| OpenAPI 3.1 | Supported; treated exactly like 3.2 |
| OpenAPI 3.0 | Supported, with the differences below |
| Swagger 2.0 (`swagger: "2.0"`) | Not supported |

In an OpenAPI 3.0 document, these keywords are handled as follows:

| Keyword | Handling |
| --- | --- |
| `nullable: true` | Adds `null` to the type. On an `enum`, `null` is added even if the list omits it. Other generators treat it the same way, even though 3.0.3 itself still disallows `null` in that case |
| `nullable: true` next to a `$ref`, or on an `allOf` with one `$ref` | Adds `null` to the referenced type |
| Other keywords next to a `$ref` | Applied, as in 3.1 and later, although 3.0 says to ignore them: `{ $ref: Base, properties: {...} }` is `Base & {...}` |
| `type: string, format: binary` | Raw binary, read as a schema without a `type`: `unknown`, or `Blob` in a multipart body or a `text/*` response |
| `type: string, format: byte` | Base64 text, read as `contentEncoding: base64`: a `string`, sent as an `application/octet-stream` multipart part |
| `example`, a boolean `exclusiveMinimum` | Ignored |

`nullable` in a 3.1 or 3.2 document is rejected, because those versions of the
specification would silently ignore it. Use `type: [..., "null"]`
instead.

## Documents

| Feature | Support | Notes |
| --- | --- | --- |
| JSON and YAML | Supported | From a file, or from stdin with `-` |
| `$ref`s to other local files | Supported | Whole documents or fragments, resolved relative to the referencing document |
| `$self`, and `$id` and `$anchor` in schemas | Supported | A `$self` URI such as `https://api.example.com/openapi/root.yaml` is mapped onto the file's own directory, so `schemas/pet.yaml` next to it is found locally |
| `components.mediaTypes` and `$ref`s to them | Supported | |
| Component names other than ASCII letters, digits, `.`, `-` and `_` | Rejected | The specification limits them to `^[a-zA-Z0-9\.\-_]+$` |
| Remote `$ref`s, and URLs as input | Not supported | `https://...` URIs that do not map onto a local file |

Schemas from other files get type aliases of their own, but only component
schemas are exported and listed in `components` (see
[Schemas](./schemas#type-aliases-and-components)).

Only generate clients from documents you trust, because oasis-gen reads any
local file that a `$ref` points to, wherever it is.

## Schemas

See [Schemas](./schemas) for how each keyword maps to a TypeScript type.

| Feature | Support | Notes |
| --- | --- | --- |
| JSON Schema 2020-12 | Supported | `type` arrays, `const`, `enum`, `allOf`, `oneOf`, `anyOf`, `prefixItems`, `minItems`, `additionalProperties`, `patternProperties`, `propertyNames` (as a list of names), `$ref` with sibling keywords, and `readOnly` and `writeOnly` (see [Strict types](./strict-types#requests-and-responses-have-separate-types)) |
| Recursive schemas | Supported | Through properties, arrays, and `additionalProperties`, including `$ref`s into `$defs` and other locations outside `components.schemas` |
| `discriminator` on `oneOf` or `anyOf` | Supported | Explicit and implicit mappings, and `defaultMapping` |
| Subtypes listed in the parent's `oneOf`, each extending it with `allOf` | Supported | See [Subtypes](./schemas#subtypes) |
| Other cycles through `$ref`, `allOf`, `oneOf` or `anyOf` alone | Rejected | Cycles with no object or array in between |
| `discriminator` on an `allOf` parent | Not supported | The parent is not turned into a union of its children |
| `$dynamicRef` | Not supported | Typed as `unknown` |

## Operations

| Feature | Support | Notes |
| --- | --- | --- |
| `get`, `put`, `post`, `delete`, `options`, `head`, `patch`, `trace` | Supported | |
| `query` (the HTTP `QUERY` method) | Supported | |
| `additionalOperations` | Supported | Custom methods such as `LINK` |
| Some `additionalOperations` names | Rejected | `delete`, `get`, `head`, `options`, `post`, and `put` regardless of case, and `PATCH`, `TRACE`, and `QUERY` in uppercase, because `fetch` would send the same method as the corresponding fixed field. `with` and `then` are also rejected because the client reserves them |
| Callbacks and webhooks | Not supported | |

## Parameters

| Location | Styles | Notes |
| --- | --- | --- |
| `query` | `form`, `spaceDelimited`, `pipeDelimited`, `deepObject` | `allowReserved` is supported |
| `path` | `simple`, `label`, `matrix` | |
| `header` | `simple` | |
| `cookie` | `form` (percent-encoded), `cookie` (sent as-is) | Exploded values are joined with `; ` for both styles, because the specification rules out `&` for cookies |
| `querystring` | Supported | Typed and encoded by its media type; see [`querystring` parameters](./client#querystring-parameters) |

Every style supports `explode`. An exploded `spaceDelimited` or `pipeDelimited`
parameter is sent like `form`, with one pair per item. Parameters with
`content` instead of a schema are also supported. JSON query parameters are
typed from their schema and serialized as JSON, while other parameters with
`content` are passed as pre-serialized strings.

Cookie parameters and cookie API keys are sent as a `Cookie` header, which
browsers ignore in `fetch`. They work in Node and Bun.

## Request and response bodies

See [Request bodies](./client#request-bodies) and
[Response bodies](./client#response-bodies) for how each media type is typed
and decoded.

| Feature | Support | Notes |
| --- | --- | --- |
| Sequential media types and `itemSchema` | Supported | JSON Lines, NDJSON, JSON text sequences, and server-sent events, streamed as async iterables |
| `multipart/form-data` request bodies | Supported | Typed by the schema and sent as `FormData`; see [Form bodies](./client#form-bodies) |
| Other `multipart` request bodies, such as `multipart/mixed` | Not supported | Passed as a `Blob` built by the caller, with `type` set to the media type and its `boundary` |
| Encoding Object (`encoding`) | Supported | `contentType`, `style`, `explode`, and `allowReserved`. `FormData` cannot send part headers or nested parts, so optional `headers` are left out, and a required header or a `multipart` part in a request body is rejected |
| `prefixEncoding`, `itemEncoding` | Not supported | They apply only to other `multipart` media types |
| Typed response headers | Not supported | |

## Servers and security

| Feature | Support | Notes |
| --- | --- | --- |
| `servers` at the top level, on path items, and on operations | Supported | Variables are set to their defaults; see [Servers and `baseUrl`](./client#servers-and-baseurl) |
| Security schemes | Supported | `apiKey`, `http`, `oauth2`, `openIdConnect`, and `mutualTLS`, through [`auth`](./authentication) |
| OAuth2 flows | Not supported | You obtain tokens yourself |
