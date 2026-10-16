# oasis

A minimal API client generator for **OpenAPI 3.2**.

oasis reads an OpenAPI 3.2 document and writes a single, dependency-free
TypeScript file: types for every operation plus a small `fetch`-based runtime.
There is no runtime package to install and nothing to configure.

```sh
oasis openapi.yaml -o src/api.ts
```

```ts
import { createClient } from "./api.ts";

const api = createClient({ headers: () => ({ authorization: `Bearer ${token}` }) });

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.ok) {
  res.data.name; // typed from the 200 response
} else {
  res.data.title; // typed from the error response
}
```

## Install

```sh
cargo install --git https://github.com/22mb/oasis
```

## Usage

```
oasis <SPEC> [-o <FILE>] [--lang <LANG>]
```

`SPEC` is a JSON or YAML file, or `-` for stdin. `$ref`s to other local files
are followed, relative to the referencing document (for stdin, relative to the
current directory). Without `-o` the client is written to stdout. `--lang`
currently accepts `typescript` (default).

## The generated client

Calls are keyed by HTTP method and path template, exactly as written in the
document. Inputs are grouped by where they go:

```ts
await api.GET("/pets", {
  query: { limit: 10, tags: ["a", "b"] },
  header: { "X-Request-Id": "r1" },
  cookie: { session: "s1" },
  signal: AbortSignal.timeout(5000), // any other fetch option
});

await api.POST("/pets", { body: { name: "Tama", kind: "cat" } });
await api.POST("/pets", {
  contentType: "application/x-www-form-urlencoded",
  body: { name: "Tama", kind: "cat" },
});
```

The init argument is optional only when the operation has no required inputs.
Every call resolves to `{ ok, status, data, response }`; `ok` and `status`
narrow `data` to the matching response type. `ok` is typed from the declared
status: `2XX` keys are successes, `default` may be either, and everything else
is an error response. Non-2xx responses are returned, not thrown. Network
errors throw as usual.

Response bodies are decoded from the response `Content-Type`:

| Media type | `data` |
| --- | --- |
| `application/json`, `*+json` | parsed JSON |
| `text/*` | `string` |
| `application/jsonl`, `application/x-ndjson`, `application/json-seq` | `AsyncIterable` of parsed items |
| `text/event-stream` | `AsyncIterable<{ data, event?, id?, retry? }>` |
| no body (`204`, `HEAD`, ...) | `undefined` |
| anything else | `Blob` |

Ranges such as `*/*` and `application/*` are typed as `unknown`, since the
actual `Content-Type` decides how they are decoded.

Client options:

```ts
createClient({
  baseUrl: "https://api.example.com/v1", // replaces every server in the document
  headers: { "x-api-key": key },          // or a (possibly async) function
  middleware: [timing, retry],            // see below
  fetch: myFetch,                         // (request: Request) => Promise<Response>
});
```

Without `baseUrl`, each operation goes to its own server (operation-level, then
path-level, then the first top-level server), with variables set to their defaults.

### Middleware

A middleware wraps the request: it may change it, call `next` zero or more
times, and returns the response. The first middleware is the outermost.

```ts
import { createClient, type Middleware } from "./api.ts";

const timing: Middleware = async (request, next, { operation }) => {
  const start = performance.now();
  try {
    return await next(request);
  } finally {
    console.log(operation.operationId ?? operation.path, performance.now() - start);
  }
};

const retry: Middleware = async (request, next) => {
  for (let attempt = 0; ; attempt++) {
    const response = await next(request.clone()); // a body can only be sent once
    if (response.status < 500 || attempt === 2) return response;
  }
};

const api = createClient({ middleware: [timing, retry] });
const scoped = api.with(tenantHeader);              // a new client; `api` is unchanged
await api.GET("/pets", { middleware: [cache] });     // runs inside the client's middleware
```

Returning a `Response` without calling `next` skips the network (caching,
mocks). Network errors surface as exceptions from `next`. The third argument
describes the operation from the document: `method`, `path` (the template),
`operationId` and its `security` requirements. Response bodies are decoded
after the middleware, so a middleware may also adjust the response.

`api.with(...)` instead of mutating a shared client keeps per-request state
(such as a tenant or a user's token on a server) from leaking between requests.

### Authentication

`auth` is a middleware that authenticates each operation as its `security`
says. You only say how to obtain each credential:

```ts
import { auth, createClient } from "./api.ts";

const api = createClient({
  middleware: [
    auth({
      bearer: (scopes) => getToken(scopes), // may be async; `undefined` if unavailable
      apiKey: process.env.API_KEY,          // a plain string works too
    }),
  ],
});
```

Keys are the names in `components.securitySchemes` (typed as
`SecuritySchemeName`). For each request, `auth` picks the first alternative
whose credentials are all available (not `undefined` or `""`), trying anonymous
access (`{}`) last; if none is, the request is sent without credentials and the
server decides. It places each credential where its scheme expects it: `Authorization: Bearer`
for `http` bearer, `oauth2` and `openIdConnect`; base64-encoded `user:password`
for `http` basic; the named header, query parameter or cookie for `apiKey`.
Operations with `security: []` get nothing. Required OAuth2 scopes are passed
to the credential function.

Component schemas are exported as `schemas`:

```ts
import type { schemas } from "./api.ts";
type Pet = schemas["Pet"];
```

`schemas` describes responses. A schema with required `readOnly` or `writeOnly`
properties also gets a request variant in `components["requestSchemas"]`, where
`readOnly` properties are optional (and `writeOnly` ones keep their
requiredness); in responses, required `writeOnly` properties are optional.

The output uses only erasable TypeScript syntax, so it also runs directly under
Node's type stripping. It type-checks with TypeScript 5.0 and later.

## OpenAPI 3.2 support

oasis accepts `openapi: 3.2.x` documents only.

- `query` operations (the HTTP `QUERY` method) and `additionalOperations` with custom methods such as `LINK`
- `in: querystring` parameters
- `itemSchema` and sequential media types (JSON Lines, NDJSON, JSON text sequences, server-sent events), streamed as async iterables
- `components.mediaTypes` and `$ref` to them
- Multi-file documents: `$ref`s to other local files (whole documents or fragments), resolved against `$self` when present, with `$id` and `$anchor` in schemas. A `$self` URI such as `https://api.example.com/openapi/root.yaml` is mapped onto the file's own directory, so `schemas/pet.yaml` next to it is found locally. Schemas from other files appear in `components["refs"]` under keys like `schemas/pet.yaml#`
- JSON Schema 2020-12 schemas: `type` arrays (`["string", "null"]`), `const`, `enum`, `allOf`/`oneOf`/`anyOf`, `prefixItems`, `additionalProperties`, `$ref` with sibling keywords, `readOnly`/`writeOnly`, recursive schemas (including `$ref`s into `$defs` and other non-component locations). Properties listed only in `required` are typed as `{} | null` (any value but `undefined`)
- `discriminator` on `oneOf`/`anyOf`, with explicit and implicit mappings, so unions narrow on the discriminating property
- Parameter styles: `form` and `deepObject` for query (with `allowReserved`), `simple` for path and header, `form` (percent-encoded) and `cookie` (sent as-is) for cookie, all with `explode`. Exploded cookie values are joined with `; ` for both styles, since the spec rules out `&` for cookies; and `content`-based parameters
- `servers` at the top level, on path items and on operations, with variable defaults

Not supported yet:

- Remote `$ref`s (`https://...` URIs that do not map onto a local file) and URLs as input
- `additionalOperations` entries for `delete`, `get`, `head`, `options`, `post` or `put` in any case are rejected, since fetch sends them uppercased and they would collide with the fixed fields
- Parameter styles `label`, `matrix`, `spaceDelimited` and `pipeDelimited` (rejected with an error)
- `multipart/mixed` streaming; multipart request bodies are passed as `FormData`
- `discriminator` on an `allOf` parent (the parent is not turned into a union of its children)
- Typed response headers, OAuth2 flows (obtaining tokens is up to you), callbacks and webhooks
- `cookie` parameters and cookie API keys are sent as a `Cookie` header, which browsers ignore in `fetch` (they work in Node, Deno and Bun)

## Design

```
JSON/YAML files ──Documents──▶ serde_json::Value per file ──parse──▶ ir::Api ──emit──▶ TypeScript
```

`src/documents.rs` loads the entry document and every local file it references,
and resolves URI references (base URIs from `$self` and `$id`). `src/parse.rs` handles everything about the OpenAPI document (`$ref`
resolution, version check, validation) and produces a language-neutral IR
(`src/ir.rs`). Schemas behind `$ref`s outside `components.schemas` are hoisted
into named schemas once, so references stay references and recursion is cheap.
Emitters only translate the IR. Adding a target language means adding one
emitter next to `src/typescript.rs`. The IR is public but not yet stable.

## Development

```sh
cargo test                      # unit tests and golden files
UPDATE_GOLDEN=1 cargo test      # regenerate tests/golden after an intended change
cd tests/ts && npm ci && npm test  # type-check and run the generated client against a test server
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
