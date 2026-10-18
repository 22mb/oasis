import assert from "node:assert/strict";
import { test } from "node:test";
import { DecodeError, createClient } from "../golden/features.ts";

type Sent = { url: string; headers: Headers };

/** A client whose fetch records the request and answers with `response`. */
function recording(response: () => Response, baseUrl?: string) {
  const sent: Sent[] = [];
  const api = createClient({
    ...(baseUrl ? { baseUrl } : {}),
    fetch: async (request: Request) => {
      sent.push({ url: request.url, headers: request.headers });
      return response();
    },
  });
  return { api, sent };
}

const json = (value: unknown) => () =>
  new Response(JSON.stringify(value), { headers: { "content-type": "application/json" } });

test("serializes exploded, reserved, JSON and cookie parameters", async () => {
  const { api, sent } = recording(json({ x: 1 }));
  const res = await api.GET("/items/{coords}", {
    path: { coords: { x: 1, y: 2 } },
    query: { path: "/a/b?c#d e%20f", filter: { a: 1 } },
    header: { "X-Meta": { a: "1", b: "2" } },
    cookie: { session: "a b;c", list: ["x", "y"], raw: "k=v" },
  });
  const [{ url, headers }] = sent as [Sent];
  assert.equal(
    url,
    "https://api.example.com/items/x=1,y=2?path=/a/b?c%23d%20e%20f&filter=%7B%22a%22%3A1%7D",
  );
  assert.equal(headers.get("x-meta"), "a=1,b=2");
  assert.equal(headers.get("cookie"), "session=a%20b%3Bc; list=x,y; raw=k=v");
  assert.deepEqual(res.data, { x: 1 }); // `*/*` decoded by the actual Content-Type
});

test("rejects path parameters that would change the path", async () => {
  const { api, sent } = recording(() => new Response(null, { status: 204 }));
  for (const dir of ["..", ".", ""]) {
    await assert.rejects(api.GET("/files/{dir}/{name}", { path: { dir, name: "a" } }), TypeError);
  }
  assert.equal(sent.length, 0);

  // Encoded dots and longer runs are ordinary names.
  await api.GET("/files/{dir}/{name}", { path: { dir: "...", name: "%2E%2E" } });
  assert.equal(sent[0]!.url, "https://api.example.com/files/.../%252E%252E");
});

test("uses path- and operation-level servers", async () => {
  const { api, sent } = recording(json({ id: 1, name: "a" }));
  await api.POST("/upload", { body: { name: "a", password: "p" } });
  await api.POST("/upload", { contentType: "application/json; charset=utf-8", body: { name: "a", password: "p" } });
  await api.GET("/shapes");
  assert.deepEqual(sent.map((s) => s.url), [
    "https://upload.example.com/upload",
    "https://upload.example.com/upload",
    "https://shapes.example.com/v2/shapes",
  ]);
  assert.equal(sent[0]!.headers.get("content-type"), "application/json");
  assert.equal(sent[1]!.headers.get("content-type"), "application/json; charset=utf-8");
});

test("baseUrl replaces every server", async () => {
  const { api, sent } = recording(json({}), "http://localhost:4010/");
  await api.POST("/upload", { body: { name: "a", password: "p" } });
  await api.GET("/shapes");
  assert.deepEqual(sent.map((s) => s.url), ["http://localhost:4010/upload", "http://localhost:4010/shapes"]);
});

test("a body that is not what its Content-Type says throws a DecodeError", async () => {
  const html = () => new Response("<html>502</html>", { status: 502, headers: { "content-type": "application/json" } });
  await assert.rejects(
    recording(html).api.GET("/shapes"),
    (e: unknown) =>
      e instanceof DecodeError && e.response.status === 502 && e.text === "<html>502</html>" &&
      e.cause instanceof SyntaxError,
  );
  // Sequential media types report the offending item while iterating.
  const lines = () => new Response('{"a":1}\nnot json\n', { headers: { "content-type": "application/jsonl" } });
  const res = await recording(lines).api.GET("/items/{coords}", { path: { coords: { x: 1 } } });
  const items: unknown[] = [];
  await assert.rejects(
    (async () => {
      for await (const item of res.data as AsyncIterable<unknown>) items.push(item);
    })(),
    (e: unknown) => e instanceof DecodeError && e.text === "not json",
  );
  assert.deepEqual(items, [{ a: 1 }]);
});

test("only the document's methods are client properties", async () => {
  const { api, sent } = recording(json({}));
  assert.equal(JSON.stringify(api), "{}");
  assert.equal(String(api), "[object Object]");
  assert.equal(Reflect.get(api, "get"), undefined); // lowercase names do not send either
  assert.equal(Reflect.get(api, "toJSON"), undefined);
  assert.equal(typeof Reflect.get(api, "GET"), "function");
  assert.equal(sent.length, 0);
  // Paths are the document's templates, which start with `/`.
  const GET = api.GET as unknown as (path: string) => Promise<unknown>;
  await assert.rejects(GET("shapes"), TypeError);
  assert.equal(sent.length, 0);
});

test("sends static and per-call headers, and is not a thenable", async () => {
  const sent: Headers[] = [];
  const api = createClient({
    headers: { "x-api-key": "k" },
    fetch: async (request: Request) => {
      sent.push(request.headers);
      return new Response(null, { status: 204 });
    },
  });
  await api.GET("/shapes", { headers: { "x-trace": "1" } });
  assert.equal(sent[0]!.get("x-api-key"), "k");
  assert.equal(sent[0]!.get("x-trace"), "1");
  // Returning the client from an async function must not look up a `then` method.
  assert.equal(Reflect.get(api, "then"), undefined);
  assert.equal(await Promise.resolve(api), api);
});

test("decodes bodies by status and Content-Type", async () => {
  // `*/*` is typed `unknown`, so every decoding applies.
  const item = (response: () => Response) =>
    recording(response).api.GET("/items/{coords}", { path: { coords: { x: 1 } } });
  const cases: [string, () => Response, unknown][] = [
    ["205", () => new Response(null, { status: 205 }), undefined],
    ["304", () => new Response(null, { status: 304 }), undefined],
    ["empty JSON", () => new Response("", { headers: { "content-type": "application/json" } }), undefined],
    ["no Content-Type", () => new Response(new TextEncoder().encode("raw")), "raw"],
    ["form", () => new Response("a=1", { headers: { "content-type": "application/x-www-form-urlencoded" } }), "a=1"],
  ];
  for (const [name, response, expected] of cases) {
    assert.deepEqual((await item(response)).data, expected, name);
  }
  const mixed = await item(() => new Response("--b--", { headers: { "content-type": "multipart/mixed; boundary=b" } }));
  assert.ok(mixed.data instanceof Blob);

  // Sequential media types by alias and by suffix.
  const collect = async (contentType: string, text: string) => {
    const res = await item(() => new Response(text, { headers: { "content-type": contentType } }));
    const items = [];
    for await (const i of res.data as AsyncIterable<unknown>) items.push(i);
    return items;
  };
  assert.deepEqual(await collect("application/x-ndjson", '{"a":1}\n{"a":2}\n'), [{ a: 1 }, { a: 2 }]);
  assert.deepEqual(await collect("application/foo+json-seq", '\x1e{"a":1}\n\x1e{"a":2}\n'), [{ a: 1 }, { a: 2 }]);
});
