# Authentication

`auth` is middleware that authenticates each operation according to its
`security`. You only need to provide a way to get each credential:

```ts twoslash
// @filename: api.ts
export * from "./security.ts";
// @filename: index.ts
declare function getToken(scopes: string[]): Promise<string | undefined>;
declare const process: { env: Record<string, string | undefined> };
// ---cut---
import { auth, createClient } from "./api.ts";

const api = createClient({
  middleware: [
    auth({
      // Keys are the scheme names in components.securitySchemes.
      bearer: (scopes) => getToken(scopes),
      // If this variable is unset, alternatives that need it are skipped.
      apiKey: process.env.API_KEY,
    }),
  ],
});
```

## Choosing credentials

The keys are the names in `components.securitySchemes` (typed as
`SecuritySchemeName`). Each value is a `Credential` (a string, or a function
that returns one, possibly asynchronously) or `undefined`. If a value is a
function, it is called for each request with the values listed in the
requirement (scopes, or role names for schemes other than OAuth 2.0 and OpenID
Connect) and the [middleware context](./middleware#the-operation). If a value
or return value is `undefined` or `""`, the credential is unavailable.

For each request, `auth` picks the first of the operation's `security`
alternatives whose credentials are all available. Alternatives that add
nothing to the request (the empty requirement `{}`, or `mutualTLS` alone) are
used only when no other alternative is available. If no alternative is
available, the request is sent without credentials, and the server decides how
to respond.

## Where credentials go

| Security scheme | Sent as |
| --- | --- |
| `oauth2`, `openIdConnect`, `http` with `bearer` | `Authorization: Bearer <credential>` |
| `http` with `basic` | `Authorization: Basic <credential, base64-encoded>`; the credential is `user:password` |
| Other `http` schemes | `Authorization: <scheme> <credential>` |
| `apiKey` in a header | The named header |
| `apiKey` in the query or a cookie | The named query parameter or cookie, percent-encoded |
| `mutualTLS` | Nothing; configure TLS on the `fetch` implementation |

Browsers ignore a `Cookie` header set in `fetch`, so cookie API keys work only
in Node and Bun.

## When no credentials are sent {#when-nothing-is-sent}

`auth` only acts on `security` in the document. It sends nothing for
operations with `security: []`, or when the document declares `securitySchemes`
without any `security` requirement. To send a header with every request
regardless of the document, use the client's `headers` option instead.
