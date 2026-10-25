# Middleware

A middleware wraps the request. It can change the request and call `next`
zero or more times, and it returns the response.

Inside a middleware, `next` works like `fetch`: it takes a `Request` and
returns a `Promise<Response>`. Whether the request then passes through other
middleware before it reaches the network is hidden from the caller.

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

<figure class="diagram">
<svg viewBox="-2 0 756 272" role="img" aria-label="A request passes through the client's middleware, timing and then retry, and the call's middleware, cache, to fetch. The response returns through them in reverse order. cache can return a response without calling fetch. Each middleware is called once: its code before next runs on the way in, and its code after next on the way out. After the chain, the body is decoded into the result." fill="currentColor" font-size="13" style="max-width: 100%; height: auto">
<defs>
<marker id="mw-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" style="fill: var(--vp-c-text-1)"/></marker>
<marker id="mw-arrow-accent" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" style="fill: var(--vp-c-brand-1)"/></marker>
</defs>
<path d="M240 50V44H470V50" fill="none" stroke="currentColor" stroke-width="1"/>
<text x="355" y="34" text-anchor="middle">client middleware</text>
<path d="M520 50V44H610V50" fill="none" stroke="currentColor" stroke-width="1"/>
<text x="565" y="34" text-anchor="middle">call middleware</text>
<rect x="0" y="60" width="110" height="84" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="55" y="107" text-anchor="middle" style="font-family: var(--vp-font-family-mono)" font-size="14">api.GET()</text>
<rect x="240" y="60" width="90" height="84" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="285" y="107" text-anchor="middle" style="font-family: var(--vp-font-family-mono)" font-size="14">timing</text>
<rect x="380" y="60" width="90" height="84" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="425" y="107" text-anchor="middle" style="font-family: var(--vp-font-family-mono)" font-size="14">retry</text>
<rect x="520" y="60" width="90" height="84" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="565" y="107" text-anchor="middle" style="font-family: var(--vp-font-family-mono)" font-size="14">cache</text>
<rect x="660" y="60" width="90" height="84" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="705" y="100" text-anchor="middle" style="font-family: var(--vp-font-family-mono)" font-size="14">fetch</text>
<text x="705" y="120" text-anchor="middle" font-size="12" style="fill: var(--vp-c-text-2)">network</text>
<line x1="110" y1="80" x2="238" y2="80" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="330" y1="80" x2="378" y2="80" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="470" y1="80" x2="518" y2="80" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="610" y1="80" x2="658" y2="80" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="660" y1="124" x2="612" y2="124" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="520" y1="124" x2="472" y2="124" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="380" y1="124" x2="332" y2="124" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<line x1="240" y1="124" x2="112" y2="124" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<text x="175" y="72" text-anchor="middle" font-size="12" style="font-family: var(--vp-font-family-mono); fill: var(--vp-c-text-2)">next(request)</text>
<text x="175" y="141" text-anchor="middle" font-size="12" style="fill: var(--vp-c-text-2)">return response</text>
<path d="M610 92C644 92 644 112 613 112" fill="none" stroke-width="2" stroke-dasharray="5 4" style="stroke: var(--vp-c-brand-1)" marker-end="url(#mw-arrow-accent)"/>
<text x="285" y="164" text-anchor="middle" font-size="12" style="fill: var(--vp-c-text-2)">outermost</text>
<text x="425" y="164" text-anchor="middle" font-size="12" style="fill: var(--vp-c-text-2)">can call next again</text>
<text x="565" y="164" text-anchor="middle" font-size="12" style="fill: var(--vp-c-brand-1)">can return early</text>
<line x1="55" y1="144" x2="55" y2="198" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<rect x="0" y="200" width="110" height="44" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="55" y="227" text-anchor="middle">decode body</text>
<line x1="110" y1="222" x2="238" y2="222" stroke="currentColor" stroke-width="1.5" marker-end="url(#mw-arrow)"/>
<rect x="240" y="200" width="280" height="44" rx="8" stroke="currentColor" stroke-width="1.5" style="fill: var(--vp-c-bg-soft)"/>
<text x="380" y="227" text-anchor="middle" style="font-family: var(--vp-font-family-mono)" font-size="14">{ ok, status, data, response }</text>
<text x="0" y="266" font-size="12" style="fill: var(--vp-c-text-2)">DecodeError is thrown here</text>
</svg>
<figcaption>How <code>api.GET()</code> passes a request through <code>[timing, retry]</code> from <code>createClient</code> and <code>[cache]</code> from the call. Each middleware is called once: the code before <code>await next(request)</code> runs on the way in, and the code after it on the way out. The highlighted path is <code>cache</code> returning a <code>Response</code> without calling <code>next</code>.</figcaption>
</figure>

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
  the body, including `DecodeError`, are not thrown from `next`.

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

Using `with` instead of mutating a shared client keeps per-request state,
such as a tenant or a user's token on a server, from leaking between requests.
