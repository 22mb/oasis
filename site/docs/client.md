# Calling operations

The generated file exports `createClient`, which returns a client with one
method for each HTTP method in the document.

## Creating a client

```ts twoslash
import { createClient } from "./api.ts";

const api = createClient({
  baseUrl: "https://api.example.com/v1",
  headers: { "x-request-source": "web" },
});
```

| Option | Description |
| --- | --- |
| `baseUrl` | Replaces every server in the document |
| `headers` | Headers sent with every request; a function (which can be async) is called for each request |
| `middleware` | Middleware that wraps every request, outermost first; see [Middleware](./middleware) |
| `fetch` | Called with the final `Request` instead of the global `fetch` |

All options are optional. To send credentials, see
[Authentication](./authentication).

### Servers and `baseUrl`

If you omit `baseUrl`, each operation is sent to its own server: the first
entry of the operation's `servers`, or else of the path item's `servers`, or
else of the top-level `servers`. Server variables are set to their defaults.

- If the document has no servers at all, paths stay relative to the current
  page. This works in browsers. In other environments, `new Request` rejects a
  relative URL, so set `baseUrl`.
- A relative `baseUrl`, such as `"/api"`, also works only in browsers.

To use a different server for each environment, see the
[recipe](./recipes#setting-the-base-url-for-each-environment).

## Calling an operation

You select an operation by its HTTP method and path template, exactly as they
are written in HTTP and in the document. The second argument groups the inputs by where they go
in the request:

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
await api.GET("/pets/{petId}", { path: { petId: 1 } });

await api.GET("/pets", {
  query: { limit: 10, tags: ["a", "b"] },
  header: { "X-Request-Id": "r1" },
  cookie: { session: "s1" },
  // Any other fetch option, such as a timeout:
  signal: AbortSignal.timeout(5000),
});

await api.POST("/pets", { body: { name: "Tama", kind: "cat" } });
```

| Field | Content |
| --- | --- |
| `path`, `query`, `header`, `cookie` | Parameters, serialized as their `style` and `explode` specify |
| `querystring` | An `in: querystring` parameter; see [below](#querystring-parameters). It replaces `query` |
| `body`, `contentType` | The request body and its media type; see [Request bodies](#request-bodies) |
| Anything else | Passed to `new Request`, except `method`. Options that `Request` does not keep, such as `next` in Next.js, do not reach `fetch` |

The second argument is optional only when the operation has no required
inputs.

Headers are applied in the following order. Later headers replace earlier
ones.

1. The client's `headers`
2. `header` parameters
3. `cookie` parameters, added to any `Cookie` header that is already set
4. The call's own `headers` option. Its `Cookie` header joins the cookies set
   so far, replacing only those of the same name

Browsers ignore a `Cookie` header set in `fetch`, so `cookie` parameters are
sent only from Node.js and Bun. A browser sends its own cookies instead.

### Request bodies

The type of `body` depends on the media type of the request:

| Media type | `body` |
| --- | --- |
| `application/json`, `*+json` | The schema type |
| `application/x-www-form-urlencoded` | The schema type, sent form-encoded |
| `multipart/form-data` | The schema type, with binary parts as `Blob`; `FormData` without a schema |
| Other `multipart/*` | `Blob`, whose `type` gives the media type with its `boundary` |
| `text/*` | `string` |
| `*/*`, `application/*` | The schema type, or `unknown` |
| Anything else | `BodyInit`, except `FormData` and `URLSearchParams` |

`contentType` selects the media type, and the type of `body` follows it. You
can omit `contentType` when the operation declares only one media type, and it
is not a range. When
the operation declares several, you can omit `contentType` to use its first
`application/json` media type. The body is then sent as that media type,
exactly as the document declares it, including its parameters (such as `charset`). For a range, `contentType` takes a
media type in it: `` `image/${string}` `` for `image/*`, any string for `*/*`.
It is required, except for `*/*` when the operation has no `application/json`
media type: left out, the body is sent as JSON, or as it is when it is a string,
`Blob`, `FormData` or another `BodyInit`.
When the request body is optional, you can omit both `body` and `contentType`.

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
await api.POST("/pets", {
  contentType: "application/x-www-form-urlencoded",
  body: { name: "Tama", kind: "cat" },
});
```

JSON bodies are serialized with `JSON.stringify`, so a string body is sent as a
JSON string when the media type is JSON. The `Content-Type` header comes from
the body's media type, so an object passed as `headers` cannot name it. For
multipart bodies, `fetch` sets the `Content-Type` header, including its
boundary, in place of any set in `headers`. A request body uses the
[request variant](./strict-types#requests-and-responses-have-separate-types)
of its schema.

A `ReadableStream` body is sent as a stream. This works in Node.js, but few
browsers can send a streamed request body (see the
[compatibility table](https://developer.mozilla.org/en-US/docs/Web/API/Request/duplex#browser_compatibility)),
so pass a `Blob` or an `ArrayBuffer` in code that also runs in browsers.

### Form bodies

A form body sends each property of the object as a field
(`application/x-www-form-urlencoded`) or a part (`multipart/form-data`). An
array sends one field or part per item, all with the property's name, and
`undefined` and `null` are left out. Each value is sent as the document's
Encoding Object for the property says, or else by its type:

| Value | Form field | Multipart part |
| --- | --- | --- |
| An object, or an array inside an array | JSON | JSON, as `application/json` |
| A `Blob` | Not supported | Binary, as the `Blob`'s own type, or `application/octet-stream` without one |
| Anything else | `String(value)` | Text |

In a multipart body, a property whose schema has no `type` is a `Blob`, since
that is how OpenAPI describes binary data. In OpenAPI 3.0,
`type: string, format: binary` means the same. A part with `type: string` is
text, also with `contentMediaType`.

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
// ---cut---
await api.POST("/pets/{petId}/photo", {
  path: { petId: 1 },
  body: { file: new File([png], "tama.png", { type: "image/png" }), caption: "Tama" },
});
```

The Encoding Object changes how a property is sent:

- `contentType` sets the media type. A JSON media type sends the value as JSON,
  also in a form field, and a multipart part is sent with that `Content-Type`.
  A `Blob` without a type gets it too. When `contentType` lists several media
  types, such as `image/png, image/jpeg`, the `Blob`'s own type says which.
- `style`, `explode`, and `allowReserved` send the property like a query
  parameter. In a multipart body, each name and value becomes a part, without
  percent-encoding.

`FormData` sends a part that has a media type, such as JSON or a `Blob`, with a
filename. Servers that treat every part with a filename as a file, such as
FastAPI's `UploadFile` and multer, therefore read JSON parts as files.

### `querystring` parameters

An `in: querystring` parameter is the whole query string. Its type and encoding
follow its media type:

| Media type | Value | Sent as |
| --- | --- | --- |
| `application/x-www-form-urlencoded` | The schema type, or `Record<string, unknown>` without a schema | Form-encoded |
| `application/json`, `*+json` | The schema type | `encodeURIComponent(JSON.stringify(value))` |
| Anything else | `string` | `encodeURIComponent(String(value))` |

## Responses

Every call resolves to `{ ok, status, data, response }`. Checking `status`
narrows `data` to the matching response type, and `ok` is `response.ok`. See
[Strict types](./strict-types#responses-are-typed-by-status) for the rules.
The client reads the body of `response` to make `data` (for sequential media
types, while you iterate), so read the body through `data`, not `response`.

### Response bodies

The client decodes a response body according to the actual `Content-Type` of
the response. The type of `data` comes from the media types that the document
declares.

| Media type | `data` |
| --- | --- |
| `application/json`, `*+json` | Parsed JSON; `undefined` for an empty body |
| `text/*`, `application/x-www-form-urlencoded` | `string`; `Blob` for a `text/*` type whose schema has no `type` |
| `application/jsonl`, `application/x-ndjson`, `application/json-seq`, `*+json-seq` | `AsyncIterable` of parsed items |
| `text/event-stream` | `AsyncIterable<{ data, event?, id?, retry? }>` |
| `*/*`, `application/*`, and `text/*` itself | Typed as `unknown`, since the range includes event streams and JSON as well as text; decoded according to the actual `Content-Type` |
| Anything else | `Blob` |

`data` is `undefined` for `HEAD` requests and for `204`, `205`, and `304`
responses. A response without a `Content-Type` is read as text, or as
`undefined` when the body is empty. A `text/*` response whose schema has no
`type` (`type: string, format: binary` in OpenAPI 3.0) is binary, such as a CSV
file to download: `data` is a `Blob`. This applies to the response that
declares it: another response with the same media type and a `type` is text.
A `text/*` response without a schema is text. Items of sequential media types are typed
from `itemSchema` when the document has one.

## Errors

Non-2xx responses are returned, not thrown. A call throws only in the
following cases.

### Network errors

Errors that `fetch` throws, such as a failed connection or an aborted request,
propagate from the call unchanged.

### `DecodeError`

A `DecodeError` is thrown when a body does not match its `Content-Type`, for
example when an HTML error page is served as `application/json`. Its `response`
property holds the response, whose body has already been consumed. Its `text`
property holds the text that failed to decode. For sequential media types, it is thrown during iteration, and `text`
is the item that failed to decode.

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
import { DecodeError } from "./api.ts";

try {
  await api.GET("/pets");
} catch (error) {
  if (error instanceof DecodeError) {
    console.error(error.response.status, error.text);
  } else {
    throw error;
  }
}
```

### `TypeError`

A `TypeError` is thrown before the request is sent when a path parameter would
make a path segment empty, `.`, or `..`.

## The client object

The client has one property for each HTTP method in the document, spelled
as in HTTP (`api.GET`, not `api.get`), plus `with` (see
[Middleware](./middleware#per-client-middleware-with-with)). Any other
property, including a lowercase method name, behaves as it does on a plain
object, so inspecting or serializing the client never sends a request.
