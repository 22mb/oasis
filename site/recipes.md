# Recipes

Each recipe uses only what the generated client already provides.

## Throwing on unexpected statuses

Calls return non-2xx responses instead of throwing. When an exception works
better, for example in a data-fetching library, wrap the call and pass the
status that you expect:

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
class HttpError extends Error {
  response: Response;
  data: unknown;
  constructor(response: Response, data: unknown) {
    super(`${response.status} ${response.statusText}`);
    this.name = "HttpError";
    this.response = response;
    this.data = data;
  }
}

type Result = {
  status: number;
  data: unknown;
  response: Response;
};

/**
 * Resolves to the data of a response with `status`,
 * and throws an HttpError for any other status.
 */
async function expectStatus<R extends Result, S extends R["status"]>(
  call: Promise<R>,
  status: S,
): Promise<Extract<R, { status: S }>["data"]> {
  const res = await call;
  if (res.status !== status) {
    throw new HttpError(res.response, res.data);
  }
  return res.data as Extract<R, { status: S }>["data"];
}

const call = api.GET("/pets/{petId}", { path: { petId: 1 } });
// `pet` is typed from the 200 response.
const pet = await expectStatus(call, 200);
```

## TanStack Query

You do not need generated hooks. Make the call in `queryFn` with the
`signal` that TanStack Query provides, so that canceled queries abort the
request.

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
class HttpError extends Error {
  response: Response;
  data: unknown;
  constructor(response: Response, data: unknown) {
    super(String(response.status));
    this.response = response;
    this.data = data;
  }
}
async function expectStatus<R extends { status: number; data: unknown; response: Response }, S extends R["status"]>(
  call: Promise<R>,
  status: S,
): Promise<Extract<R, { status: S }>["data"]> {
  const res = await call;
  if (res.status !== status) throw new HttpError(res.response, res.data);
  return res.data as Extract<R, { status: S }>["data"];
}
// ---cut---
import { useQuery } from "@tanstack/react-query";

function usePet(petId: number) {
  return useQuery({
    queryKey: ["pets", petId],
    queryFn: ({ signal }) => {
      const call = api.GET("/pets/{petId}", {
        path: { petId },
        signal,
      });
      return expectStatus(call, 200);
    },
  });
}
```

## Setting the base URL for each environment

To call a different server in each environment, for example when the
document's servers point to a development host, set `baseUrl` from your build's
environment variables. With Vite:

```ts twoslash
import { createClient } from "./api.ts";
declare global {
  interface ImportMeta {
    env: { VITE_API_BASE_URL: string };
  }
}
// ---cut---
const api = createClient({ baseUrl: import.meta.env.VITE_API_BASE_URL });
```

In the browser, a relative `baseUrl` such as `"/api"` also works, for example
with the `server.proxy` option of Vite's dev server.

## Refreshing a token on 401

A middleware can call `next` more than once. To retry a request with a new
token after a `401` response, keep an unsent copy of the request:

```ts twoslash
import { createClient, type Middleware } from "./api.ts";
// ---cut---
declare function signIn(): Promise<string>;

let token = await signIn();

const bearer: Middleware = async (request, next) => {
  // A body can only be sent once, so keep a copy for the retry.
  const retry = request.clone();
  request.headers.set("authorization", `Bearer ${token}`);
  const response = await next(request);
  if (response.status !== 401) return response;
  token = await signIn();
  retry.headers.set("authorization", `Bearer ${token}`);
  return next(retry);
};

const api = createClient({ middleware: [bearer] });
```

## Mocking responses in tests

The `fetch` option replaces the network for the whole client:

```ts twoslash
import { createClient } from "./api.ts";
// ---cut---
const pet = { id: 1, name: "Tama", kind: "cat" };

const api = createClient({
  baseUrl: "https://api.test",
  fetch: async () => Response.json(pet),
});
```

To mock some operations and send the other requests over the network, use a
middleware that returns a `Response` without calling `next`:

```ts twoslash
import { createClient, type Middleware } from "./api.ts";
const api = createClient();
// ---cut---
const mock: Middleware = async (request, next, { operation }) => {
  if (operation.method === "GET" && operation.path === "/pets") {
    return Response.json([{ id: 1, name: "Tama", kind: "cat" }]);
  }
  return next(request);
};

const mocked = api.with(mock);
```

## Reading streamed responses

For a response with a sequential media type, such as JSON Lines or
server-sent events, `data` is an async iterable. Narrow on `status` first, and
then iterate:

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
const res = await api.GET("/stream/jsonl");
if (res.status === 200) {
  for await (const pet of res.data) {
    console.log(pet.name);
  }
}
```

Breaking out of the loop stops the download. If an item does not parse, the
loop throws a `DecodeError`.

## Per-request clients on a server

On a server, one client can serve requests from many users. Create the client
once, and use `with` to derive a client for each incoming request, so that one
user's token never reaches another user's request:

```ts twoslash
import { createClient } from "./api.ts";
// ---cut---
const base = createClient({ baseUrl: "https://api.example.com" });

async function handle(incoming: Request): Promise<Response> {
  const token = incoming.headers.get("authorization");
  const api = base.with(async (request, next) => {
    if (token) request.headers.set("authorization", token);
    return next(request);
  });
  const res = await api.GET("/pets");
  return Response.json(res.status === 200 ? res.data : []);
}
```

## Managing the generated file {#keeping-the-generated-file-in-your-project}

If you commit the generated file, the project can type-check without running
oasis-gen. If you generate it in a build step instead, it stays in sync with
the document. In either case, exclude the file from linters and formatters,
because it does not follow your project's style rules. For example, with
ESLint's flat config:

```js
import { globalIgnores } from "eslint/config";

export default [
  globalIgnores(["src/api.ts"]),
  // ...your config
];
```

## Troubleshooting {#common-problems}

| Problem | Cause | Solution |
| --- | --- | --- |
| `TypeError: Failed to parse URL` outside a browser | The request URL is relative, because the document has no servers or `baseUrl` is relative | Set an absolute `baseUrl` |
| `TS5097: An import path can only end with a '.ts' extension ...` | The project does not allow `.ts` extensions in imports | Enable `allowImportingTsExtensions`, or import `./api.js` or `./api` |
| `TS2554: Expected 0-1 arguments, but got 2.` in `DecodeError` | `lib` is older than ES2022 | Set `lib` to ES2022 or later |
| `TS2304: Cannot find name 'RequestInit'`, `'Blob'`, or `'FormData'` | Neither the `dom` lib nor `@types/node` is available | Add one of them |
| `DecodeError` for an error response | The server returned a body that does not match its `Content-Type`, such as an HTML error page | Catch `DecodeError` and check `error.response.status`; see [Errors](./client#decodeerror) |
| Cookie parameters or cookie API keys are not sent in a browser | Browsers ignore a `Cookie` header set in `fetch` | Let the browser send its own cookies, for example with the `credentials` fetch option |
| `nullable` is rejected | The document is OpenAPI 3.1 or 3.2, where `nullable` does not exist | Use `type: [..., "null"]` |
