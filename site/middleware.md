# Middleware

A middleware wraps the request. It can change the request and call `next`
zero or more times, and it returns the response.

```ts twoslash
import { createClient, type Middleware } from "./api.ts";

const timing: Middleware = async (request, next, { operation }) => {
  const start = performance.now();
  try {
    return await next(request);
  } finally {
    const name = operation.operationId ?? operation.path;
    console.log(name, performance.now() - start);
  }
};

const retry: Middleware = async (request, next) => {
  for (let attempt = 0; ; attempt++) {
    // A body can only be sent once, so send a copy.
    const response = await next(request.clone());
    if (response.status < 500 || attempt === 2) return response;
  }
};

const api = createClient({ middleware: [timing, retry] });
```

## How the chain runs

- The first middleware is the outermost. Middleware passed to a single call
  (`api.GET("/pets", { middleware: [...] })`) runs inside the client's
  middleware.
- `next` passes the request to the next middleware. In the last middleware,
  `next` passes it to `fetch`. If a middleware returns a `Response` without
  calling `next`, the request is not sent over the network. This is useful for
  caching or mocks.
- Errors that `fetch` itself throws are thrown from `next`.
- Response bodies are read and decoded after the chain finishes. As a result,
  a middleware can adjust the response, and errors that occur while reading
  the body, including `DecodeError`, do not pass through `next`.

## The operation

The third argument is `{ operation }`, which describes the operation from the
document:

| Field | Content |
| --- | --- |
| `method` | The HTTP method, as used in the call |
| `path` | The path template, such as `/pets/{petId}` |
| `operationId` | The `operationId`, when the document has one |
| `security` | The security requirements that apply, in document order |

## Per-client middleware with `with`

`api.with(...middleware)` returns a new client whose middleware runs inside
the original client's middleware. `api` itself does not change.

```ts twoslash
import { createClient, type Middleware } from "./api.ts";
const api = createClient();
// ---cut---
const tenant = (id: string): Middleware => (request, next) => {
  request.headers.set("x-tenant-id", id);
  return next(request);
};

const acme = api.with(tenant("acme"));
await acme.GET("/pets");
```

Using `with` instead of changing a shared client keeps per-request state,
such as a tenant or a user's token on a server, from leaking between requests.
