import { expect, test } from "vitest";
import { DecodeError, createClient, type ClientOptions } from "../golden/features.ts";
import { cookieHeaders } from "./env.ts";
import { json, noContent, server, type Sent } from "./fetch.ts";

/** A client whose fetch records the request and answers with `response`. */
function recording(response: () => Response = noContent, options: ClientOptions = {}) {
  const { fetch, sent } = server(response);
  return { api: createClient({ ...options, fetch }), sent };
}

test("serializes exploded, reserved, JSON and cookie parameters", async () => {
  const { api, sent } = recording(json({ x: 1 }));
  const res = await api.GET("/items/{coords}", {
    path: { coords: { x: 1, y: 2 } },
    query: { path: "/a/b?c#d e%20f", filter: { a: 1 } },
    header: { "X-Meta": { a: "1", b: "2" } },
    cookie: { session: "a b;c", list: ["x", "y"], raw: "k=v" },
  });
  const [{ url, headers }] = sent as [Sent];
  expect(url).toBe("https://api.example.com/items/x=1,y=2?path=/a/b?c%23d%20e%20f&filter=%7B%22a%22%3A1%7D");
  expect(headers.get("x-meta")).toBe("a=1,b=2");
  if (cookieHeaders) expect(headers.get("cookie")).toBe("session=a%20b%3Bc; list=x,y; raw=k=v");
  expect(res.data).toStrictEqual({ x: 1 }); // `*/*` decoded by the actual Content-Type
});

test("rejects path parameters that would change the path", async () => {
  const { api, sent } = recording();
  for (const dir of ["..", ".", ""]) {
    await expect(api.GET("/files/{dir}/{name}", { path: { dir, name: "a" } })).rejects.toThrow(TypeError);
  }
  expect(sent.length).toBe(0);

  // Encoded dots and longer runs are ordinary names.
  await api.GET("/files/{dir}/{name}", { path: { dir: "...", name: "%2E%2E" } });
  expect(sent[0]!.url).toBe("https://api.example.com/files/.../%252E%252E");
});

test("uses path- and operation-level servers", async () => {
  const { api, sent } = recording(json({ id: 1, name: "a" }));
  await api.POST("/upload", { body: { name: "a", password: "p" } });
  await api.POST("/upload", { contentType: "application/json; charset=utf-8", body: { name: "a", password: "p" } });
  await api.GET("/shapes");
  expect(sent.map((s) => s.url)).toStrictEqual([
    "https://upload.example.com/upload",
    "https://upload.example.com/upload",
    "https://shapes.example.com/v2/shapes",
  ]);
  // The declared media type is sent, parameters included.
  expect(sent[0]!.headers.get("content-type")).toBe("application/json; charset=utf-8");
  expect(sent[1]!.headers.get("content-type")).toBe("application/json; charset=utf-8");
});

test("serializes a querystring parameter as its media type", async () => {
  const { api, sent } = recording();
  await api.GET("/lookup", { querystring: { tags: ["a b", "#c"] } });
  await api.POST("/lookup", { querystring: "x=1&y=#2" });
  expect(sent.map((s) => s.url)).toStrictEqual([
    "https://api.example.com/lookup?%7B%22tags%22%3A%5B%22a%20b%22%2C%22%23c%22%5D%7D",
    "https://api.example.com/lookup?x%3D1%26y%3D%232",
  ]);
});

test("sends a body as the operation's media type when none is specified", async () => {
  const { api, sent } = recording();
  // A string is JSON when the operation takes JSON, not text/plain.
  await api.POST("/notes", { body: "hi" });
  await api.PATCH("/notes", { body: { text: "hi" } });
  expect(sent.map((s) => [s.headers.get("content-type"), s.body])).toStrictEqual([
    ["application/json", '"hi"'],
    ["application/merge-patch+json", '{"text":"hi"}'],
  ]);
});

test("baseUrl replaces every server", async () => {
  const { api, sent } = recording(json({}), { baseUrl: "http://localhost:4010/" });
  await api.POST("/upload", { body: { name: "a", password: "p" } });
  await api.GET("/shapes");
  expect(sent.map((s) => s.url)).toStrictEqual(["http://localhost:4010/upload", "http://localhost:4010/shapes"]);
});

test("throws a DecodeError when a body does not match its Content-Type", async () => {
  const html = () => new Response("<html>502</html>", { status: 502, headers: { "content-type": "application/json" } });
  const error = await recording(html).api.GET("/shapes").catch((e: unknown) => e);
  expect(error).toBeInstanceOf(DecodeError);
  expect(error).toMatchObject({ response: { status: 502 }, text: "<html>502</html>" });
  expect((error as DecodeError).cause).toBeInstanceOf(SyntaxError);
  // Sequential media types report the offending item while iterating.
  const lines = () => new Response('{"a":1}\nnot json\n', { headers: { "content-type": "application/jsonl" } });
  const res = await recording(lines).api.GET("/items/{coords}", { path: { coords: { x: 1 } } });
  const items: unknown[] = [];
  const iterating = (async () => {
    for await (const item of res.data as AsyncIterable<unknown>) items.push(item);
  })();
  const itemError = await iterating.catch((e: unknown) => e);
  expect(itemError).toBeInstanceOf(DecodeError);
  expect(itemError).toMatchObject({ text: "not json" });
  expect(items).toStrictEqual([{ a: 1 }]);
});

test("only the document's methods are client properties", async () => {
  const { api, sent } = recording(json({}));
  expect(JSON.stringify(api)).toBe("{}");
  expect(String(api)).toBe("[object Object]");
  expect(Reflect.get(api, "get")).toBe(undefined); // lowercase names do not send either
  expect(Reflect.get(api, "toJSON")).toBe(undefined);
  expect(typeof Reflect.get(api, "GET")).toBe("function");
  expect(Object.keys(api)).toContain("GET"); // a plain property, so a test can replace it
  expect(sent.length).toBe(0);
  // Paths are the document's templates, which start with `/`.
  const GET = api.GET as unknown as (path: string) => Promise<unknown>;
  await expect(GET("shapes")).rejects.toThrow(TypeError);
  await expect(GET("//evil.example/x")).rejects.toThrow(TypeError); // a host in a relative URL
  expect(sent.length).toBe(0);
});

test("sends static and per-call headers, and is not a thenable", async () => {
  const { api, sent } = recording(noContent, {
    headers: { "x-api-key": "k" },
  });
  await api.GET("/shapes", { headers: { "x-trace": "1" } });
  expect(sent[0]!.headers.get("x-api-key")).toBe("k");
  expect(sent[0]!.headers.get("x-trace")).toBe("1");
  // Returning the client from an async function must not look up a `then` method.
  expect(Reflect.get(api, "then")).toBe(undefined);
  expect(await Promise.resolve(api)).toBe(api);
});

test("reads a text/* response declared as binary in the document as a Blob", async () => {
  const report = async (status: number, contentType: string) => {
    const response = () => new Response("a,b\n", { status, headers: { "content-type": contentType } });
    return (await recording(response).api.GET("/report")).data;
  };
  const csv = await report(200, "text/csv; charset=Shift_JIS");
  expect(csv).toBeInstanceOf(Blob);
  expect(await (csv as Blob).text()).toBe("a,b\n");
  // The same media type is read as text in another response that declares it as text.
  expect(await report(400, "TEXT/CSV")).toBe("a,b\n");
  expect(await report(200, "text/plain")).toBe("a,b\n");
  expect(await report(200, "text/html")).toBe("a,b\n");
  // A binary range covers every media type in it, and no others...
  const exported = async (contentType: string, status = 200) => {
    const response = () => new Response("1", { status, headers: { "content-type": contentType } });
    return (await recording(response).api.GET("/exports")).data as unknown;
  };
  expect(await exported("text/markdown; charset=utf-8")).toBeInstanceOf(Blob);
  expect(await exported("application/json")).toBe(1);
  // ...only in the responses it is declared for.
  expect(await exported("text/plain", 404)).toBe("1");
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
    expect((await item(response)).data, name).toStrictEqual(expected);
  }
  const mixed = await item(() => new Response("--b--", { headers: { "content-type": "multipart/mixed; boundary=b" } }));
  expect(mixed.data).toBeInstanceOf(Blob);

  // Sequential media types by alias and by suffix.
  const collect = async (contentType: string, text: string) => {
    const res = await item(() => new Response(text, { headers: { "content-type": contentType } }));
    const items = [];
    for await (const i of res.data as AsyncIterable<unknown>) items.push(i);
    return items;
  };
  expect(await collect("application/x-ndjson", '{"a":1}\n{"a":2}\n')).toStrictEqual([{ a: 1 }, { a: 2 }]);
  expect(await collect("application/foo+json-seq", '\x1e{"a":1}\n\x1e{"a":2}\n')).toStrictEqual([{ a: 1 }, { a: 2 }]);
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
  expect(url).toBe("https://operation.example.com/params/a%20b,c/x,1,y,2?a=1&b=2&flat=a,1,b,2&deep%5Ba%5D=1&deep%5Ba%5D=2&deep%5Bb%5D=3");
  expect(headers.get("x-ids")).toBe("1,2");
  expect(headers.get("x-pair")).toBe("a,1,b,2");
  if (cookieHeaders) expect(headers.get("cookie")).toBe("multi=x; multi=y");
  expect(res.data).toStrictEqual({ n: 1 });

  // `undefined` also omits a parameter, which can be passed without `exactOptionalPropertyTypes`.
  const absent = undefined as never;
  await api.GET("/params/{list}/{pair}", { path: { list: ["a"], pair: { x: 1 } }, query: { obj: absent }, header: { "X-Ids": absent } });
  expect(sent[1]!.url).toBe("https://operation.example.com/params/a/x,1");
  expect(sent[1]!.headers.get("x-ids")).toBe(null);
});

test("serializes label, matrix and delimited styles", async () => {
  const { api, sent } = recording();
  await api.GET("/styles/{label}/{labels}/{matrix}/{matrices}", {
    path: { label: ["a b", "c"], labels: { x: 1, y: 2 }, matrix: ["a", "b"], matrices: { x: 1, y: "" } },
    query: { spaced: ["a b", "c"], piped: { a: 1, b: 2 }, each: ["a", "b"] },
  });
  await api.POST("/uploads", { contentType: "application/x-www-form-urlencoded", body: { labels: ["a", "b|c"] } });
  expect(sent.map((s) => [s.url, s.body])).toStrictEqual([
    // An empty matrix value is sent as its name alone; an exploded delimited style is `form`.
    ["https://api.example.com/styles/.a%20b,c/.x=1.y=2/;matrix=a,b/;x=1;y?spaced=a%20b%20c&piped=a%7C1%7Cb%7C2&each=a&each=b", ""],
    ["https://api.example.com/uploads", "labels=a%7Cb%7Cc"],
  ]);
});

test("adds cookie parameters to a cookie from async client headers", async () => {
  if (!cookieHeaders) return;
  const { api, sent } = recording(noContent, {
    headers: async () => ({ cookie: "theme=dark", "x-api-key": "k" }),
  });
  await api.GET("/params/{list}/{pair}", { path: { list: ["a"], pair: { x: 1 } }, cookie: { multi: ["x"] } });
  expect(sent[0]!.headers.get("cookie")).toBe("theme=dark; multi=x");
  expect(sent[0]!.headers.get("x-api-key")).toBe("k");
});

test("form bodies repeat array values and leave out null", async () => {
  const { api, sent } = recording();
  await api.PUT("/forms/{id}", { path: { id: 1 }, contentType: "application/x-www-form-urlencoded", body: { tags: ["a", "b c"], note: null } });
  await api.PUT("/forms/{id}", { path: { id: 1 }, body: { tags: ["a"], note: null } });
  expect(sent.map((s) => [s.url, s.headers.get("content-type"), s.body])).toStrictEqual([
    ["https://api.example.com/forms/1", "application/x-www-form-urlencoded", "tags=a&tags=b+c"],
    ["https://api.example.com/forms/1", "application/json", '{"tags":["a"],"note":null}'],
  ]);
});

test("serializes object form fields as JSON and follows Encoding Objects", async () => {
  const { api, sent } = recording();
  await api.POST("/uploads", {
    contentType: "application/x-www-form-urlencoded",
    body: { meta: { tags: ["a"], note: null }, filter: { kind: "cat", size: 2 }, path: "/a b?c=d", id: "i 1" },
  });
  await api.PUT("/forms/{id}", { path: { id: 1 }, contentType: "application/x-www-form-urlencoded", body: { tags: [{ a: 1 } as unknown as string] } });
  expect(sent.map((s) => s.body)).toStrictEqual([
    "meta=%7B%22tags%22%3A%5B%22a%22%5D%2C%22note%22%3Anull%7D&kind=cat&size=2&path=/a%20b?c=d&id=%22i+1%22",
    "tags=%7B%22a%22%3A1%7D",
  ]);
});

/** Each part of a multipart body: its name, media type (without parameters) and content. */
function parts({ headers, body }: Sent): [string, string | null, string][] {
  const boundary = headers.get("content-type")!.match(/^multipart\/form-data; boundary=(.+)$/)![1]!;
  return body.split(`--${boundary}`).slice(1, -1).map((part) => {
    const [head = "", content = ""] = part.split("\r\n\r\n");
    const name = head.match(/ name="([^"]*)"/)![1]!;
    const type = head.match(/content-type: ([^;\r]+)/i)?.[1] ?? null;
    return [name, type, content.slice(0, -2)];
  });
}

test("multipart parts take their media type from the value or the Encoding Object", async () => {
  const { api, sent } = recording();
  await api.POST("/uploads", {
    contentType: "multipart/form-data",
    body: {
      file: new File(["f"], "a.jpg", { type: "image/jpeg" }),
      attachments: [new Blob(["x"]), new Blob(["y"])],
      photo: new Blob(["p"]),
      meta: { tags: ["a"] },
      caption: "Tama",
      checksum: "iQ==",
      tags: ["a", "b"],
      filter: { kind: "cat" },
      note: "n",
    },
  });
  expect(sent[0]!.body).toMatch(/name="file"; filename="a.jpg"/);
  expect(parts(sent[0]!)).toStrictEqual([
    // A Blob keeps its own type; if it has none, it is sent as the single media type
    // in its Encoding Object, or as application/octet-stream by default.
    ["file", "image/jpeg", "f"],
    ["attachments", "application/octet-stream", "x"],
    ["attachments", "application/octet-stream", "y"],
    ["photo", "image/png", "p"],
    ["meta", "application/json", '{"tags":["a"]}'],
    ["caption", null, "Tama"],
    ["checksum", "application/octet-stream", "iQ=="],
    ["tags", null, "a"],
    ["tags", null, "b"],
    ["filter[kind]", null, "cat"],
    ["note", "application/json", '"n"'],
  ]);
});

test("a multipart body replaces an existing Content-Type so fetch can add the boundary", async () => {
  const { api, sent } = recording(noContent, {
    // Plain header objects cannot specify Content-Type, but `Headers` and arrays can.
    headers: new Headers({ "Content-Type": "application/json" }),
  });
  const body = { file: new Blob(["f"]), meta: {} };
  await api.POST("/uploads", { contentType: "multipart/form-data", body });
  await api.POST("/uploads", { contentType: "multipart/form-data", body, headers: [["Content-Type", "multipart/form-data"]] });
  for (const { headers } of sent) expect(headers.get("content-type")!).toMatch(/^multipart\/form-data; boundary=/);
});

test("infers the media type from the body for a media type range unless specified", async () => {
  const { api, sent } = recording();
  await api.POST("/blobs", { body: new TextEncoder().encode("raw") });
  await api.POST("/blobs", { body: { a: 1 } });
  await api.POST("/blobs", { contentType: "text/csv", body: "a,b" });
  await api.PUT("/avatar", { contentType: "image/png", body: new Blob(["png"]) }); // `image/*`
  expect(sent.map((s) => [s.headers.get("content-type"), s.body])).toStrictEqual([
    [null, "raw"],
    ["application/json", '{"a":1}'],
    ["text/csv", "a,b"],
    ["image/png", "png"],
  ]);
});

test("sends a schema-less multipart/form-data FormData body as-is", async () => {
  const { api, sent } = recording();
  const body = new FormData();
  body.append("a", "1");
  await api.POST("/attachments", { body });
  expect(parts(sent[0]!)).toStrictEqual([["a", null, "1"]]);
});

test("a multipart body other than multipart/form-data is a Blob that carries its Content-Type", async () => {
  const { api, sent } = recording();
  const type = "multipart/mixed; boundary=b";
  await api.POST("/batch", { contentType: "multipart/mixed", body: new Blob(["--b--"], { type }) });
  expect(sent.map((s) => [s.headers.get("content-type"), s.body])).toStrictEqual([[type, "--b--"]]);
});

test("a form querystring takes fields or an encoded string", async () => {
  const { api, sent } = recording();
  await api.GET("/blobs", { querystring: { a: 1, b: [2, 3], c: null } });
  // A form querystring with a string schema is passed as-is, with any leading "?" stripped.
  await api.GET("/blobs", { querystring: "?a=1&b=%202" as unknown as Record<string, unknown> });
  expect(sent.map((s) => s.url)).toStrictEqual([
    "https://api.example.com/blobs?a=1&b=2&b=3",
    "https://api.example.com/blobs?a=1&b=%202",
  ]);
});

test("an optional body can be omitted without specifying a media type", async () => {
  const { api, sent } = recording();
  await api.PUT("/photos/{id}", { path: { id: 1 } });
  await api.PUT("/photos/{id}", { path: { id: 1 }, contentType: "image/png", body: new Uint8Array([1]) });
  expect(sent.map((s) => [s.headers.get("content-type"), s.body.length])).toStrictEqual([
    [null, 0],
    ["image/png", 1],
  ]);
});

test("merges a per-call Cookie header with cookie parameters", async () => {
  if (!cookieHeaders) return;
  const { api, sent } = recording(noContent, {
    headers: { cookie: "theme=dark" },
  });
  const path = { list: ["a"], pair: { x: 1 } };
  await api.GET("/params/{list}/{pair}", { path, cookie: { multi: ["x"] }, headers: { cookie: "lang=ja; token=a=b" } });
  // A matching cookie name replaces the earlier value; other headers with the same name are still overwritten.
  await api.GET("/params/{list}/{pair}", { path, cookie: { multi: ["x"] }, headers: { cookie: "theme=light", "x-ids": "9" }, header: { "X-Ids": [1] } });
  expect(sent.map(({ headers }) => [headers.get("cookie"), headers.get("x-ids")])).toStrictEqual([
    ["theme=dark; multi=x; lang=ja; token=a=b", null],
    ["multi=x; theme=light", "9"],
  ]);
});
