import assert from "node:assert/strict";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, test } from "node:test";
import { createClient } from "../golden/petstore.ts";

type Recorded = { method: string; url: string; headers: IncomingMessage["headers"]; body: Buffer };
let last: Recorded;
const tama = { id: 1, name: "Tama", kind: "cat", tag: null };

const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
  const chunks: Buffer[] = [];
  for await (const c of req) chunks.push(c as Buffer);
  last = { method: req.method!, url: req.url!, headers: req.headers, body: Buffer.concat(chunks) };
  const json = (status: number, value: unknown, type = "application/json") => {
    res.writeHead(status, { "content-type": type }).end(JSON.stringify(value));
  };
  const route = `${req.method} ${req.url!.split("?")[0]}`;
  const stream = async (type: string, parts: string[]) => {
    res.writeHead(200, { "content-type": type });
    for (const p of parts) {
      res.write(p);
      await new Promise((r) => setTimeout(r, 5));
    }
    res.end();
  };
  switch (route) {
    case "GET /v1/pets": return json(200, [tama]);
    case "POST /v1/pets":
      return req.headers["content-type"] === "application/json"
        ? json(201, { id: 2, ...JSON.parse(last.body.toString()) })
        : json(201, { id: 3 });
    case "QUERY /v1/pets": return json(200, [tama]);
    case "HEAD /v1/pets": return res.writeHead(200, { "content-type": "application/json", "x-total": "1" }).end();
    case "GET /v1/pets/1": return json(200, tama);
    case "GET /v1/pets/2": return json(404, { title: "Not Found" }, "application/problem+json");
    case "DELETE /v1/pets/1":
    case "LINK /v1/pets/1":
    case "PUT /v1/pets/1/photo": return res.writeHead(204).end();
    case "POST /v1/pets/1/photo":
      return req.headers["content-type"]?.startsWith("multipart/form-data; boundary=")
        ? res.writeHead(204).end()
        : json(415, { title: "expected multipart/form-data" });
    case "GET /v1/pets/1/photo": return res.writeHead(200, { "content-type": "image/png" }).end(Buffer.from([0x89, 0x50]));
    case "GET /v1/search": return res.writeHead(200, { "content-type": "text/plain" }).end("ok");
    case "GET /v1/stream/jsonl":
      return stream("application/jsonl", ['{"id":1,"na', 'me":"a"}\n{"id":2,"name":"b"}\n', "\n"]);
    case "GET /v1/stream/seq":
      return stream("application/json-seq", ['\x1e{"id":1}\n\x1e{"id"', ':2}\n']);
    case "GET /v1/stream/sse":
      return stream("text/event-stream", [
        "retry: 10\n\n: comment\r\nevent: greet\r\ndata: hello\r\ndata: wor",
        "ld\r\nid: 7\r\n\r\ndata: {\"n\":1}\n\nid: 8\n\ndata: x\n\n",
        "data: trailing-without-blank-line",
      ]);
    default:
      return json(500, { title: `unexpected ${route}` });
  }
});

let api: ReturnType<typeof createClient>;
before(async () => {
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const { port } = server.address() as AddressInfo;
  api = createClient({ baseUrl: `http://127.0.0.1:${port}/v1/`, headers: () => ({ authorization: "Bearer t" }) });
});
after(() => server.close());

test("serializes path, query, header and cookie parameters", async () => {
  const res = await api.GET("/pets", {
    query: { limit: 2, tags: ["a b", "c"], ids: [1, 2], filter: { kind: "cat", minAge: 3 } },
    header: { "X-Request-Id": "r1" },
    cookie: { session: "s1" },
  });
  assert.equal(res.ok, true);
  assert.deepEqual(res.data, [tama]);
  assert.equal(last.url, "/v1/pets?limit=2&tags=a%20b&tags=c&ids=1,2&filter%5Bkind%5D=cat&filter%5BminAge%5D=3");
  assert.equal(last.headers["x-request-id"], "r1");
  assert.equal(last.headers.cookie, "session=s1");
  assert.equal(last.headers.authorization, "Bearer t");
});

test("sends JSON and form bodies", async () => {
  const created = await api.POST("/pets", { body: { name: "Mike", kind: "cat" } });
  assert.equal(created.status, 201);
  assert.equal(last.headers["content-type"], "application/json");
  assert.deepEqual(created.data, { id: 2, name: "Mike", kind: "cat" });

  await api.POST("/pets", { contentType: "application/x-www-form-urlencoded", body: { name: "Mi ke", kind: "cat" } });
  assert.equal(last.headers["content-type"], "application/x-www-form-urlencoded");
  assert.equal(last.body.toString(), "name=Mi+ke&kind=cat");
});

test("supports QUERY and additionalOperations methods", async () => {
  const found = await api.QUERY("/pets", { body: { name: "Tama" } });
  assert.equal(last.method, "QUERY");
  assert.deepEqual(JSON.parse(last.body.toString()), { name: "Tama" });
  assert.deepEqual(found.data, [tama]);

  const linked = await api.LINK("/pets/{petId}", { path: { petId: 1 }, header: { Link: "</owners/1>" } });
  assert.equal(last.method, "LINK");
  assert.equal(last.headers.link, "</owners/1>");
  assert.equal(linked.status, 204);
  assert.equal(linked.data, undefined);
});

test("returns error responses as data", async () => {
  const res = await api.GET("/pets/{petId}", { path: { petId: 2 } });
  assert.equal(res.ok, false);
  assert.equal(res.status, 404);
  assert.deepEqual(res.data, { title: "Not Found" });
});

test("sends querystring parameters", async () => {
  const res = await api.GET("/search", { querystring: { term: "a b", page: 2 } });
  assert.equal(last.url, "/v1/search?term=a+b&page=2");
  assert.equal(res.data, "ok");
});

test("handles binary bodies", async () => {
  await api.PUT("/pets/{petId}/photo", { path: { petId: 1 }, contentType: "image/png", body: new Uint8Array([1, 2, 3]) });
  assert.equal(last.headers["content-type"], "image/png");
  assert.deepEqual([...last.body], [1, 2, 3]);

  const photo = await api.GET("/pets/{petId}/photo", { path: { petId: 1 } });
  assert.ok(photo.data instanceof Blob);
  assert.deepEqual([...new Uint8Array(await photo.data.arrayBuffer())], [0x89, 0x50]);
});

test("streams request bodies", async () => {
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array([1, 2]));
      controller.enqueue(new Uint8Array([3]));
      controller.close();
    },
  });
  const res = await api.PUT("/pets/{petId}/photo", { path: { petId: 1 }, contentType: "image/png", body });
  assert.equal(res.status, 204);
  assert.deepEqual([...last.body], [1, 2, 3]);
});

test("an aborted signal cancels the request", async () => {
  await assert.rejects(api.GET("/pets", { signal: AbortSignal.abort() }), { name: "AbortError" });
});

test("sends multipart bodies with the boundary set by fetch", async () => {
  const form = { caption: "Tama", file: new File([new Uint8Array([0x89, 0x50])], "tama.png", { type: "image/png" }) };
  const sentAsMultipart = () => {
    assert.match(last.headers["content-type"]!, /^multipart\/form-data; boundary=/);
    assert.match(last.body.toString(), /name="caption"\r\n\r\nTama\r\n/);
    assert.match(last.body.toString(), /name="file"; filename="tama.png"\r\ncontent-type: image\/png/i);
  };
  const implied = await api.POST("/pets/{petId}/photo", { path: { petId: 1 }, body: form });
  assert.equal(implied.status, 204);
  sentAsMultipart();
  await api.POST("/pets/{petId}/photo", { path: { petId: 1 }, contentType: "multipart/form-data", body: form });
  sentAsMultipart();
});

test("HEAD responses have no body", async () => {
  const res = await api.HEAD("/pets");
  assert.equal(last.method, "HEAD");
  assert.equal(res.status, 200);
  assert.equal(res.ok, true);
  assert.equal(res.data, undefined);
  assert.equal(res.response.headers.get("x-total"), "1");
});

test("streams JSON Lines and JSON text sequences", async () => {
  const lines = await api.GET("/stream/jsonl");
  assert.ok(lines.status === 200);
  const items = [];
  for await (const item of lines.data) items.push(item);
  assert.deepEqual(items, [{ id: 1, name: "a" }, { id: 2, name: "b" }]);

  const seq = await api.GET("/stream/seq");
  assert.ok(seq.status === 200);
  const records = [];
  for await (const item of seq.data) records.push(item);
  assert.deepEqual(records, [{ id: 1 }, { id: 2 }]);
});

test("streams server-sent events", async () => {
  const res = await api.GET("/stream/sse");
  assert.ok(res.status === 200);
  const events = [];
  for await (const e of res.data) events.push(e);
  // `retry` and `id` persist: the second event has no `id` line of its own, and the
  // `id: 8` block dispatches nothing but applies to the event after it.
  assert.deepEqual(events, [
    { retry: 10, event: "greet", id: "7", data: "hello\nworld" },
    { retry: 10, id: "7", data: '{"n":1}' },
    { retry: 10, id: "8", data: "x" },
  ]);
});

test("stops reading when the consumer breaks early", async () => {
  // A body that never ends on its own: only `cancel()` stops the download.
  let cancelled = 0;
  const endless = () =>
    new ReadableStream<Uint8Array>({
      pull(controller) {
        controller.enqueue(new TextEncoder().encode('{"id":1,"name":"a"}\n'));
      },
      cancel() {
        cancelled++;
      },
    });
  const local = createClient({
    baseUrl: "http://localhost/",
    fetch: async () => new Response(endless(), { headers: { "content-type": "application/jsonl" } }),
  });

  const res = await local.GET("/stream/jsonl");
  assert.ok(res.status === 200);
  for await (const item of res.data) {
    assert.deepEqual(item, { id: 1, name: "a" });
    break;
  }
  assert.equal(cancelled, 1);

  // Throwing inside the loop cancels too.
  const failing = await local.GET("/stream/jsonl");
  assert.ok(failing.status === 200);
  await assert.rejects(
    (async () => {
      for await (const item of failing.data) throw new Error(`stop at ${item.id}`);
    })(),
    { message: "stop at 1" },
  );
  assert.equal(cancelled, 2);
});
