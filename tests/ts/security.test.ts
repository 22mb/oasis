import { expect, test } from "vitest";
import { auth, createClient, type Middleware, type MiddlewareContext } from "../golden/security.ts";
import { cookieHeaders } from "./env.ts";
import { noContent, server } from "./fetch.ts";

test("runs middleware outermost first, with the operation from the document", async () => {
  const s = server(noContent);
  const log: string[] = [];
  const contexts: MiddlewareContext[] = [];
  const named = (name: string): Middleware => async (request, next, context) => {
    log.push(`>${name}`);
    contexts.push(context);
    const response = await next(request);
    log.push(`<${name}`);
    return response;
  };
  const api = createClient({ fetch: s.fetch, middleware: [named("a"), named("b")] });
  await api.GET("/pets", { middleware: [named("call")] });
  expect(log).toStrictEqual([">a", ">b", ">call", "<call", "<b", "<a"]);
  expect(contexts[0]!.operation).toStrictEqual({
    method: "GET",
    path: "/pets",
    operationId: "listPets",
    security: [{}, { bearer: [] }],
  });

  await api.GET("/me");
  expect(contexts.at(-1)!.operation).toStrictEqual({ method: "GET", path: "/me", security: [{ bearer: [] }] });
});

test("middleware can answer without sending", async () => {
  const s = server(noContent);
  const cached: Middleware = async () => Response.json(["cached"]);
  const api = createClient({ fetch: s.fetch, middleware: [cached] });
  const res = await api.GET("/pets");
  expect(res.data).toStrictEqual(["cached"]);
  expect(s.sent.length).toBe(0);
});

test("middleware can retry with a cloned request", async () => {
  const s = server(() => new Response(null, { status: 503 }), noContent);
  const retry: Middleware = async (request, next) => {
    for (let attempt = 0; ; attempt++) {
      const response = await next(request.clone());
      if (response.status < 500 || attempt === 2) return response;
    }
  };
  const api = createClient({ fetch: s.fetch, middleware: [retry] });
  const res = await api.POST("/login", { body: { user: "tama" } });
  expect(res.status).toBe(204);
  expect(s.sent.map((r) => r.body)).toStrictEqual(['{"user":"tama"}', '{"user":"tama"}']);
});

test("with() derives a client without changing the original", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch });
  const tenant = api.with(async (request, next) => {
    request.headers.set("x-tenant", "t1");
    return next(request);
  });
  await tenant.GET("/me");
  await api.GET("/me");
  expect(s.sent[0]!.headers.get("x-tenant")).toBe("t1");
  expect(s.sent[1]!.headers.get("x-tenant")).toBe(null);
});

test("auth follows the document's security requirements", async () => {
  const s = server(() => Response.json([]), noContent);
  const scopesSeen: string[][] = [];
  let token: string | undefined = "t0k3n";
  const api = createClient({
    fetch: s.fetch,
    middleware: [
      auth({
        bearer: () => token,
        oauth: (scopes) => {
          scopesSeen.push(scopes);
          return "oauth-token";
        },
        apiKey: "k3y",
        session: "s1",
        basic: "user:päss",
        digest: "d1g3st",
        oidc: "0idc",
      }),
    ],
  });
  /** The headers of the one request that `call` sends. */
  const sent = async (call: () => Promise<unknown>) => {
    const before = s.sent.length;
    await call();
    expect(s.sent.length).toBe(before + 1);
    return s.sent.at(-1)!.headers;
  };
  let headers: Headers;

  headers = await sent(() => api.POST("/login", { body: { user: "tama" } })); // security: []
  expect(headers.get("authorization")).toBe(null);

  headers = await sent(() => api.GET("/pets")); // optional: authenticated when a token is available
  expect(headers.get("authorization")).toBe("Bearer t0k3n");
  token = undefined;
  headers = await sent(() => api.GET("/pets"));
  expect(headers.get("authorization")).toBe(null);
  token = "t0k3n";

  headers = await sent(() => api.GET("/me")); // inherits the top-level requirement
  expect(headers.get("authorization")).toBe("Bearer t0k3n");

  headers = await sent(() => api.DELETE("/admin/users"));
  expect(headers.get("authorization")).toBe("Bearer oauth-token");
  expect(scopesSeen).toStrictEqual([["admin", "write"]]);

  headers = await sent(() => api.POST("/webhooks"));
  expect(headers.get("x-api-key")).toBe("k3y");
  expect(headers.get("authorization")).toBe(null);

  headers = await sent(() => api.GET("/reports")); // no queryKey credential: falls back to basic
  expect(headers.get("authorization")).toBe(`Basic ${btoa(String.fromCharCode(...new TextEncoder().encode("user:päss")))}`);

  headers = await sent(() => api.GET("/both")); // both schemes together
  expect(headers.get("x-api-key")).toBe("k3y");
  if (cookieHeaders) expect(headers.get("cookie")).toBe("sid=s1");

  headers = await sent(() => api.GET("/digest")); // other http schemes: `<scheme> <credential>`
  expect(String(headers.get("authorization"))).toMatch(/^digest d1g3st$/i);

  headers = await sent(() => api.GET("/oidc"));
  expect(headers.get("authorization")).toBe("Bearer 0idc");

  headers = await sent(() => api.GET("/mtls")); // satisfied by the TLS configuration: nothing to add
  expect(headers.get("authorization")).toBe(null);
  headers = await sent(() => api.GET("/mtls-or-bearer")); // `mutualTLS` alone adds nothing: the bearer alternative wins
  expect(headers.get("authorization")).toBe("Bearer t0k3n");

  headers = await sent(() => api.GET("/by-uri")); // a requirement naming the scheme by its URI
  expect(headers.get("authorization")).toBe("Bearer t0k3n");

  // With no satisfiable alternative, the request goes out unauthenticated.
  token = undefined;
  headers = await sent(() => api.GET("/me"));
  expect(headers.get("authorization")).toBe(null);
  token = ""; // an empty credential counts as unavailable
  headers = await sent(() => api.GET("/me"));
  expect(headers.get("authorization")).toBe(null);
});

test("auth treats an undefined credential as unavailable", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ queryKey: undefined, basic: "u:p" })] });
  await api.GET("/reports"); // the queryKey alternative is skipped for basic
  expect(s.sent[0]!.url).toBe("https://api.example.com/reports");
  expect(s.sent[0]!.headers.get("authorization")).toBe(`Basic ${btoa("u:p")}`);
});

test("auth awaits credential functions", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ bearer: async () => "t" })] });
  await api.GET("/me");
  expect(s.sent[0]!.headers.get("authorization")).toBe("Bearer t");
});

test("auth appends query API keys without rewriting the rest of the request", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ queryKey: "q k" })] });
  await api.GET("/reports", { query: { ids: [1, 2] } }); // `explode: false` keeps its comma
  expect(s.sent[0]!.url).toBe("https://api.example.com/reports?ids=1,2&api_key=q%20k");

  const res = await api.POST("/reports", { body: { q: "x" } });
  expect(res.status).toBe(204);
  expect(s.sent[1]!.url).toBe("https://api.example.com/reports?api_key=q%20k");
  expect(s.sent[1]!.method).toBe("POST");
  expect(s.sent[1]!.headers.get("content-type")).toBe("application/json");
  expect(s.sent[1]!.body).toBe('{"q":"x"}');
});

test("auth sets cookie API keys once, encoded, replacing an existing cookie", async () => {
  if (!cookieHeaders) return;
  const s = server(noContent);
  const twice: Middleware = async (request, next) => {
    await next(request);
    return next(request); // the same Request object again, as a careless retry would
  };
  const api = createClient({ fetch: s.fetch, middleware: [twice, auth({ apiKey: "k", session: "s 1;x" })] });
  await api.GET("/both", { headers: { cookie: "sid=old; theme=dark" } });
  expect(s.sent[0]!.headers.get("cookie")).toBe("theme=dark; sid=s%201%3Bx");
  expect(s.sent[1]!.headers.get("cookie")).toBe("theme=dark; sid=s%201%3Bx");
});

test("auth sends a resent request with only the credentials chosen for it", async () => {
  // The same Request object is sent twice, so record what each send carried.
  const sent: [string, string | null][] = [];
  const fetch = async (request: Request) => {
    const url = new URL(request.url);
    sent.push([url.pathname + url.search, request.headers.get("authorization")]);
    return new Response(null, { status: 401 });
  };
  const retry: Middleware = async (request, next) => {
    const first = await next(request);
    return first.status === 401 ? next(request) : first;
  };
  // Basic first; then the query key, which comes first in the requirements.
  let tries = 0;
  const reports = createClient({
    fetch,
    middleware: [retry, auth({ queryKey: () => (tries++ === 0 ? undefined : "k"), basic: "u:p" })],
  });
  await reports.GET("/reports");
  // A token, then none: the request goes without credentials.
  let token: string | undefined = "t1";
  const pets = createClient({
    fetch: async (request) => {
      const response = await fetch(request);
      token = undefined;
      return response;
    },
    middleware: [retry, auth({ bearer: () => token })],
  });
  await pets.GET("/pets");
  expect(sent).toStrictEqual([
    ["/reports", "Basic dTpw"],
    ["/reports?api_key=k", null],
    ["/pets", "Bearer t1"],
    ["/pets", null],
  ]);
});

test("synchronous throws reach middleware as rejections", async () => {
  const fallback: Middleware = (request, next) => next(request).catch(() => Response.json([]));
  const broken: Middleware = () => {
    throw new Error("middleware threw synchronously");
  };
  const api = createClient({
    fetch: () => {
      throw new TypeError("fetch threw synchronously");
    },
    middleware: [fallback],
  });
  expect((await api.GET("/pets")).data).toStrictEqual([]);
  expect((await api.GET("/pets", { middleware: [broken] })).data).toStrictEqual([]);
});
