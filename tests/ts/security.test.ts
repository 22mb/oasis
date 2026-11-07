import { expect, test } from "vitest";
import { auth, createClient, type Middleware, type MiddlewareContext } from "../golden/security.ts";
import { cookieHeaders } from "./env.ts";

/** A fetch that records requests and answers with `responses` in turn (the last one repeats). */
function server(...responses: (() => Response)[]) {
  const requests: Request[] = [];
  const bodies: string[] = [];
  const fetch = async (request: Request) => {
    requests.push(request);
    bodies.push(await request.text());
    return (responses[requests.length - 1] ?? responses.at(-1)!)();
  };
  return { fetch, requests, bodies };
}
const noContent = () => new Response(null, { status: 204 });

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
  expect(s.requests.length).toBe(0);
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
  expect(s.bodies).toStrictEqual(['{"user":"tama"}', '{"user":"tama"}']);
});

test("middleware can handle network errors", async () => {
  const fallback: Middleware = async (request, next) => {
    try {
      return await next(request);
    } catch {
      return Response.json([]);
    }
  };
  const api = createClient({
    fetch: async () => {
      throw new TypeError("network down");
    },
    middleware: [fallback],
  });
  expect((await api.GET("/pets")).data).toStrictEqual([]);
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
  expect(s.requests[0]!.headers.get("x-tenant")).toBe("t1");
  expect(s.requests[1]!.headers.get("x-tenant")).toBe(null);
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
  const header = (i: number, name: string) => s.requests[i]!.headers.get(name);

  await api.POST("/login", { body: { user: "tama" } }); // security: []
  expect(header(0, "authorization")).toBe(null);

  await api.GET("/pets"); // optional: authenticated when a token is available
  expect(header(1, "authorization")).toBe("Bearer t0k3n");
  token = undefined;
  await api.GET("/pets");
  expect(header(2, "authorization")).toBe(null);
  token = "t0k3n";

  await api.GET("/me"); // inherits the top-level requirement
  expect(header(3, "authorization")).toBe("Bearer t0k3n");

  await api.DELETE("/admin/users");
  expect(header(4, "authorization")).toBe("Bearer oauth-token");
  expect(scopesSeen).toStrictEqual([["admin", "write"]]);

  await api.POST("/webhooks");
  expect(header(5, "x-api-key")).toBe("k3y");
  expect(header(5, "authorization")).toBe(null);

  await api.GET("/reports"); // no queryKey credential: falls back to basic
  expect(header(6, "authorization")).toBe(`Basic ${btoa(String.fromCharCode(...new TextEncoder().encode("user:päss")))}`);

  await api.GET("/both"); // both schemes together
  expect(header(7, "x-api-key")).toBe("k3y");
  if (cookieHeaders) expect(header(7, "cookie")).toBe("sid=s1");

  await api.GET("/digest"); // other http schemes: `<scheme> <credential>`
  expect(String(header(8, "authorization"))).toMatch(/^digest d1g3st$/i);

  await api.GET("/oidc");
  expect(header(9, "authorization")).toBe("Bearer 0idc");

  await api.GET("/mtls"); // satisfied by the TLS configuration: nothing to add
  expect(header(10, "authorization")).toBe(null);
  await api.GET("/mtls-or-bearer"); // `mutualTLS` alone adds nothing: the bearer alternative wins
  expect(header(11, "authorization")).toBe("Bearer t0k3n");

  await api.GET("/by-uri"); // a requirement naming the scheme by its URI
  expect(header(12, "authorization")).toBe("Bearer t0k3n");

  // With no satisfiable alternative, the request goes out unauthenticated.
  token = undefined;
  await api.GET("/me");
  expect(header(13, "authorization")).toBe(null);
  token = ""; // an empty credential counts as unavailable
  await api.GET("/me");
  expect(header(14, "authorization")).toBe(null);
});

test("auth treats an undefined credential as unavailable", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ queryKey: undefined, basic: "u:p" })] });
  await api.GET("/reports"); // the queryKey alternative is skipped for basic
  expect(s.requests[0]!.url).toBe("https://api.example.com/reports");
  expect(s.requests[0]!.headers.get("authorization")).toBe(`Basic ${btoa("u:p")}`);
});

test("auth awaits credential functions", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ bearer: async () => "t" })] });
  await api.GET("/me");
  expect(s.requests[0]!.headers.get("authorization")).toBe("Bearer t");
});

test("auth appends query API keys without rewriting the rest of the request", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ queryKey: "q k" })] });
  await api.GET("/reports", { query: { ids: [1, 2] } }); // `explode: false` keeps its comma
  expect(s.requests[0]!.url).toBe("https://api.example.com/reports?ids=1,2&api_key=q%20k");

  const res = await api.POST("/reports", { body: { q: "x" } });
  expect(res.status).toBe(204);
  expect(s.requests[1]!.url).toBe("https://api.example.com/reports?api_key=q%20k");
  expect(s.requests[1]!.method).toBe("POST");
  expect(s.requests[1]!.headers.get("content-type")).toBe("application/json");
  expect(s.bodies[1]).toBe('{"q":"x"}');
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
  expect(s.requests[0]!.headers.get("cookie")).toBe("theme=dark; sid=s%201%3Bx");
  expect(s.requests[1]!.headers.get("cookie")).toBe("theme=dark; sid=s%201%3Bx");
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
