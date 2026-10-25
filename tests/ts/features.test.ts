import assert from "node:assert/strict";
import { test } from "node:test";
import { DecodeError, createClient } from "../golden/features.ts";

type Sent = { url: string; headers: Headers; body: string };

/** A client whose fetch records the request and answers with `response`. */
function recording(response: () => Response, baseUrl?: string) {
  const sent: Sent[] = [];
  const api = createClient({
    ...(baseUrl ? { baseUrl } : {}),
    fetch: async (request: Request) => {
      sent.push({ url: request.url, headers: request.headers, body: await request.text() });
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
  // The declared media type is sent, parameters included.
  assert.equal(sent[0]!.headers.get("content-type"), "application/json; charset=utf-8");
  assert.equal(sent[1]!.headers.get("content-type"), "application/json; charset=utf-8");
});

test("serializes a querystring parameter as its media type", async () => {
  const { api, sent } = recording(() => new Response(null, { status: 204 }));
  await api.GET("/lookup", { querystring: { tags: ["a b", "#c"] } });
  await api.POST("/lookup", { querystring: "x=1&y=#2" });
  assert.deepEqual(sent.map((s) => s.url), [
    "https://api.example.com/lookup?%7B%22tags%22%3A%5B%22a%20b%22%2C%22%23c%22%5D%7D",
    "https://api.example.com/lookup?x%3D1%26y%3D%232",
  ]);
});

test("a body is sent as the operation's media type when the call names none", async () => {
  const { api, sent } = recording(() => new Response(null, { status: 204 }));
  // A string is JSON when the operation takes JSON, not text/plain.
  await api.POST("/notes", { body: "hi" });
  await api.PATCH("/notes", { body: { text: "hi" } });
  assert.deepEqual(sent.map((s) => [s.headers.get("content-type"), s.body]), [
    ["application/json", '"hi"'],
    ["application/merge-patch+json", '{"text":"hi"}'],
  ]);
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
  await assert.rejects(GET("//evil.example/x"), TypeError); // a host in a relative URL
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

test("serializes arrays and objects in every location", async () => {
  const { api, sent } = recording(json({ n: 1 }));
  const res = await api.GET("/params/{list}/{pair}", {
    path: { list: ["a b", "c"], pair: { x: 1, y: 2 } },
    query: { obj: { a: 1, b: 2 }, flat: { a: 1, b: 2 }, deep: { a: [1, 2], b: [3] }, opt: null },
    header: { "X-Ids": [1, 2], "X-Pair": { a: "1", b: "2" } },
    cookie: { multi: ["x", "y"] },
  });
  const [{ url, headers }] = sent as [Sent];
  // The operation's server wins over the path's; `null` leaves a parameter out.
  assert.equal(
    url,
    "https://operation.example.com/params/a%20b,c/x,1,y,2?a=1&b=2&flat=a,1,b,2&deep%5Ba%5D=1&deep%5Ba%5D=2&deep%5Bb%5D=3",
  );
  assert.equal(headers.get("x-ids"), "1,2");
  assert.equal(headers.get("x-pair"), "a,1,b,2");
  assert.equal(headers.get("cookie"), "multi=x; multi=y");
  assert.deepEqual(res.data, { n: 1 });

  // `undefined` leaves a parameter out too, as passed without `exactOptionalPropertyTypes`.
  const absent = undefined as never;
  await api.GET("/params/{list}/{pair}", { path: { list: ["a"], pair: { x: 1 } }, query: { obj: absent }, header: { "X-Ids": absent } });
  assert.equal(sent[1]!.url, "https://operation.example.com/params/a/x,1");
  assert.equal(sent[1]!.headers.get("x-ids"), null);
});

test("adds cookie parameters to a cookie from async client headers", async () => {
  const sent: Headers[] = [];
  const api = createClient({
    headers: async () => ({ cookie: "theme=dark", "x-api-key": "k" }),
    fetch: async (request: Request) => {
      sent.push(request.headers);
      return new Response(null, { status: 204 });
    },
  });
  await api.GET("/params/{list}/{pair}", { path: { list: ["a"], pair: { x: 1 } }, cookie: { multi: ["x"] } });
  assert.equal(sent[0]!.get("cookie"), "theme=dark; multi=x");
  assert.equal(sent[0]!.get("x-api-key"), "k");
});

test("form bodies repeat array values and leave out null", async () => {
  const { api, sent } = recording(() => new Response(null, { status: 204 }));
  await api.PUT("/forms/{id}", { path: { id: 1 }, contentType: "application/x-www-form-urlencoded", body: { tags: ["a", "b c"], note: null } });
  await api.PUT("/forms/{id}", { path: { id: 1 }, body: { tags: ["a"], note: null } });
  assert.deepEqual(sent.map((s) => [s.url, s.headers.get("content-type"), s.body]), [
    ["https://api.example.com/forms/1", "application/x-www-form-urlencoded", "tags=a&tags=b+c"],
    ["https://api.example.com/forms/1", "application/json", '{"tags":["a"],"note":null}'],
  ]);
});

test("a body for a media type range is sent by its kind unless the call names one", async () => {
  const { api, sent } = recording(() => new Response(null, { status: 204 }));
  await api.POST("/blobs", { body: new TextEncoder().encode("raw") });
  await api.POST("/blobs", { body: { a: 1 } });
  await api.POST("/blobs", { contentType: "text/csv", body: "a,b" });
  assert.deepEqual(sent.map((s) => [s.headers.get("content-type"), s.body]), [
    [null, "raw"],
    ["application/json", '{"a":1}'],
    ["text/csv", "a,b"],
  ]);
});

test("a form querystring takes fields or an encoded string", async () => {
  const { api, sent } = recording(() => new Response(null, { status: 204 }));
  await api.GET("/blobs", { querystring: { a: 1, b: [2, 3], c: null } });
  // A form querystring whose schema is a string is passed as it is, without a leading "?".
  await api.GET("/blobs", { querystring: "?a=1&b=%202" as unknown as Record<string, unknown> });
  assert.deepEqual(sent.map((s) => s.url), [
    "https://api.example.com/blobs?a=1&b=2&b=3",
    "https://api.example.com/blobs?a=1&b=%202",
  ]);
});
