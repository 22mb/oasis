import assert from "node:assert/strict";
import { test } from "node:test";
import { auth, createClient, type Middleware, type MiddlewareContext } from "../golden/security.ts";

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
  assert.deepEqual(log, [">a", ">b", ">call", "<call", "<b", "<a"]);
  assert.deepEqual(contexts[0]!.operation, {
    method: "GET",
    path: "/pets",
    operationId: "listPets",
    security: [{}, { bearer: [] }],
  });

  await api.GET("/me");
  assert.deepEqual(contexts.at(-1)!.operation, { method: "GET", path: "/me", security: [{ bearer: [] }] });
});

test("middleware can answer without sending", async () => {
  const s = server(noContent);
  const cached: Middleware = async () => Response.json(["cached"]);
  const api = createClient({ fetch: s.fetch, middleware: [cached] });
  const res = await api.GET("/pets");
  assert.deepEqual(res.data, ["cached"]);
  assert.equal(s.requests.length, 0);
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
  assert.equal(res.status, 204);
  assert.deepEqual(s.bodies, ['{"user":"tama"}', '{"user":"tama"}']);
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
  assert.deepEqual((await api.GET("/pets")).data, []);
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
  assert.equal(s.requests[0]!.headers.get("x-tenant"), "t1");
  assert.equal(s.requests[1]!.headers.get("x-tenant"), null);
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
      }),
    ],
  });
  const header = (i: number, name: string) => s.requests[i]!.headers.get(name);

  await api.POST("/login", { body: { user: "tama" } }); // security: []
  assert.equal(header(0, "authorization"), null);

  await api.GET("/pets"); // optional: authenticated when a token is available
  assert.equal(header(1, "authorization"), "Bearer t0k3n");
  token = undefined;
  await api.GET("/pets");
  assert.equal(header(2, "authorization"), null);
  token = "t0k3n";

  await api.GET("/me"); // inherits the top-level requirement
  assert.equal(header(3, "authorization"), "Bearer t0k3n");

  await api.DELETE("/admin/users");
  assert.equal(header(4, "authorization"), "Bearer oauth-token");
  assert.deepEqual(scopesSeen, [["admin", "write"]]);

  await api.POST("/webhooks");
  assert.equal(header(5, "x-api-key"), "k3y");
  assert.equal(header(5, "authorization"), null);

  await api.GET("/reports"); // no queryKey credential: falls back to basic
  assert.equal(header(6, "authorization"), `Basic ${Buffer.from("user:päss").toString("base64")}`);

  await api.GET("/both"); // both schemes together
  assert.equal(header(7, "x-api-key"), "k3y");
  assert.equal(header(7, "cookie"), "sid=s1");
});

test("auth places query API keys in the URL", async () => {
  const s = server(noContent);
  const api = createClient({ fetch: s.fetch, middleware: [auth({ queryKey: "q k" })] });
  await api.GET("/reports");
  assert.equal(s.requests[0]!.url, "https://api.example.com/reports?api_key=q+k");
});
