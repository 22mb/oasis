import assert from "node:assert/strict";
import { test } from "node:test";
import { createClient } from "../golden/features.ts";

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
