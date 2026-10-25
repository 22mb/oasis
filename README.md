# oasis-gen

A minimal API client generator for **OpenAPI 3.2**.

oasis-gen reads an OpenAPI 3.0, 3.1 or 3.2 document and writes a single,
dependency-free TypeScript file: types for every operation plus a small
`fetch`-based runtime.
There is no runtime package to install and nothing to configure.

```sh
npx oasis-gen openapi.yaml -o src/api.ts
```

```ts
import { createClient } from "./api.ts";

const api = createClient({ headers: () => ({ authorization: `Bearer ${token}` }) });

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.status === 200) {
  res.data.name; // typed from the 200 response
} else if (res.status === 404) {
  res.data.title; // typed from the 404 response
}
```

## Install

```sh
npm install --save-dev oasis-gen
```

Node.js 24 or later is supported, both for the CLI and for the generated
client (which also runs in browsers, Deno and Bun).

The package ships prebuilt binaries for macOS (arm64, x64), Linux (arm64, x64;
static, so glibc and musl both work) and Windows (arm64, x64); npm installs
only the one for your platform. Archives are also attached to each GitHub
release. To build from source instead:

```sh
cargo install --git https://github.com/22mb/oasis
```

## Usage

```
oasis-gen <SPEC> [-o <FILE>] [--lang <LANG>]
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
`contentType` picks the request media type. It may be left out when the body
has only one media type, or for its first `application/json` one, and the body
is then sent as that media type, exactly as declared (a string is JSON when the
operation takes JSON).
Every call resolves to `{ ok, status, data, response }`, and `status` narrows
`data` to the matching response type. A status code key covers that code, a
range such as `4XX` the codes in it not declared on their own, and `default`
every other status, successes included. Statuses that no key covers are still
possible, as the specification allows, and have `data: unknown`. `ok` is
`response.ok`, so after `if (res.ok)` `data` also includes `default` and
undeclared successes; compare `status` (or `switch` on it) to get the declared
type. Non-2xx responses are returned, not thrown. Network
errors throw as usual. A body that is not what its `Content-Type` says (an
HTML error page served as `application/json`, say) throws a `DecodeError`,
which carries the `response` (body consumed) and the offending `text`. A path
parameter that would leave a path segment empty, `.` or `..` is rejected with
a `TypeError` before anything is sent.

The client has one property per HTTP method in the document, plus `with`.
Any other property, including a lowercase method name, is what a plain object
has, so inspecting or serializing the client never sends a request. `with` and
`then` are therefore not allowed as `additionalOperations` names.

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
mocks). Errors from `fetch` itself surface as exceptions from `next`; the body
is read after the chain, so errors while reading it, `DecodeError` included,
do not pass through `next`. The third argument
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
whose credentials are all available (not `undefined` or `""`). Alternatives
that add nothing to the request (anonymous `{}`, or `mutualTLS` alone) are
taken only when no other is; if none is, the request is sent without
credentials and the server decides. It places each credential where its scheme
expects it: `Authorization: Bearer` for `http` bearer, `oauth2` and
`openIdConnect`; base64-encoded `user:password` for `http` basic; the named
header, or the percent-encoded named query parameter or cookie, for `apiKey`.
Operations with `security: []` get nothing. Required OAuth2 scopes are passed
to the credential function.

Component schemas are exported as `schemas`:

```ts
import type { schemas } from "./api.ts";
type Pet = schemas["Pet"];
```

`schemas` describes responses. A schema with `readOnly` or `writeOnly`
properties also gets a request variant in `components["requestSchemas"]`. As
the specification says they should not be sent, `readOnly` properties are
absent from requests (`id?: never`) and `writeOnly` ones from responses; the
other side keeps the declared requiredness. A property is also `readOnly` when
its `$ref` target is, or when another `allOf` member marks a property of the
same name, so `allOf: [$ref Base, { required: [id] }]` keeps `id` out of
requests when `Base` marks it `readOnly`.

Each schema becomes a type alias named after it, `$Pet` (and `$Pet$Request`
for the request variant); that is the name hovers and error messages show.
`schemas` and `components` map the document's names onto these aliases.

The output uses only erasable TypeScript syntax, so it also runs directly under
Node's type stripping. It type-checks with TypeScript 5.0 and later, and does
not need the `dom` lib.

## OpenAPI support

oasis-gen accepts `openapi: 3.0.x`, `3.1.x` and `3.2.x` documents. A 3.1
document is read exactly like a 3.2 one. In a 3.0 document, `nullable: true`
adds `null` to the type, also to an `enum` (as other generators read it,
although 3.0.3 itself leaves an `enum` without `null` excluding it);
`example`, a boolean `exclusiveMinimum` and `format: binary` or `byte` do not
affect the generated types and are ignored. `nullable` in a 3.1 or 3.2
document is rejected, since the spec would have it silently ignored there.

The 3.2 features oasis-gen supports:

- `query` operations (the HTTP `QUERY` method) and `additionalOperations` with custom methods such as `LINK`
- `in: querystring` parameters
- `itemSchema` and sequential media types (JSON Lines, NDJSON, JSON text sequences, server-sent events), streamed as async iterables
- `components.mediaTypes` and `$ref` to them
- Multi-file documents: `$ref`s to other local files (whole documents or fragments), resolved against `$self` when present, with `$id` and `$anchor` in schemas. A `$self` URI such as `https://api.example.com/openapi/root.yaml` is mapped onto the file's own directory, so `schemas/pet.yaml` next to it is found locally. Schemas from other files appear in `components["refs"]` under keys like `schemas/pet.yaml#`
- JSON Schema 2020-12 schemas: `type` arrays (`["string", "null"]`), `const`, `enum`, `allOf`/`oneOf`/`anyOf`, `prefixItems`, `additionalProperties`, `$ref` with sibling keywords, `readOnly`/`writeOnly`, recursive schemas (through properties, arrays and `additionalProperties`, including `$ref`s into `$defs` and other non-component locations). Properties listed only in `required` are typed as `{} | null` (any value but `undefined`)
- `discriminator` on `oneOf`/`anyOf`, with explicit and implicit mappings, so unions narrow on the discriminating property
- A schema that lists its subtypes in `oneOf` while each subtype extends it with `allOf` (or a bare `$ref`), as springdoc emits for Jackson subtypes: the parent is the union of its subtypes, and each subtype gets the parent's own properties instead of a reference back, which TypeScript could not resolve. Any other cycle through `$ref`, `allOf`, `oneOf` or `anyOf` alone, with no object or array in between, is rejected
- Parameter styles: `form` and `deepObject` for query (with `allowReserved`), `simple` for path and header, `form` (percent-encoded) and `cookie` (sent as-is) for cookie, all with `explode`. Exploded cookie values are joined with `; ` for both styles, since the spec rules out `&` for cookies; and `content`-based parameters
- `servers` at the top level, on path items and on operations, with variable defaults

Not supported yet:

- Swagger 2.0 documents (`swagger: "2.0"`)
- `$dynamicRef` (read as `unknown`)
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
