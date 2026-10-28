// ---------------------------------------------------------------------------
// Runtime (embedded by oasis-gen; identical in every generated file)
// ---------------------------------------------------------------------------

// `@types/node` declares `RequestInit` globally but not lib.dom's `HeadersInit`
// and `BodyInit`, so derive them from it: the file then type-checks without the
// dom lib. `Content-Type` comes from the body's media type (`contentType`), so a
// header object cannot name it; `Headers` and arrays are not checked.
type HeadersInit = NonNullable<RequestInit["headers"]> & { "Content-Type"?: never; "content-type"?: never };
type BodyInit = NonNullable<RequestInit["body"]>;

export interface ClientOptions {
  /**
   * Prepended to every path, replacing every server in the document. Defaults to
   * the first top-level server, or the operation's own server when it declares one.
   */
  baseUrl?: string;
  /** Sent with every request; a function is called per request. */
  headers?: HeadersInit | (() => HeadersInit | Promise<HeadersInit>);
  /** Wraps every request, outermost first. See {@link Middleware}. */
  middleware?: Middleware[];
  /** Custom fetch implementation, called with the final `Request`. */
  fetch?: (request: Request) => Promise<Response>;
}

/** Standard fetch options accepted by every operation, besides its parameters and body. */
export type RequestOptions = Omit<RequestInit, "method" | "body" | "headers"> & {
  headers?: HeadersInit;
  /** Runs inside the client's middleware, for this call only. */
  middleware?: Middleware[];
};

/**
 * The result of calling operation `O` of `paths`. One member per declared
 * response key, each with the statuses it covers: a code itself, a range such
 * as `4XX` minus the codes declared in it, and `default` every other status,
 * successes included. Statuses nothing covers are still possible and have
 * `data: unknown`. `ok` is `response.ok`, so narrow on `status` to tell a
 * declared success from a `default` or undeclared one.
 */
export type ApiResponse<O extends Operation> = {
  [K in keyof O["statuses"]]: O["statuses"][K] extends [infer Ok, infer Failed]
    ? Member<Ok, true, Data<O, K>> | Member<Failed, false, Data<O, K>>
    : never;
}[keyof O["statuses"]];

type Data<O extends Operation, K> = K extends keyof O["responses"] ? O["responses"][K] : unknown;
type Member<S, Ok, D> = [S] extends [never] ? never : { status: S; ok: Ok; data: D; response: Response };

/**
 * One alternative of an operation's security: every scheme in it applies together,
 * with the listed scopes. `{}` allows anonymous access.
 */
export type SecurityRequirement = { [K in SecuritySchemeName]?: string[] };

/** What the document says about the operation being called. */
export type OperationInfo = {
  method: string;
  /** Path template, e.g. `/pets/{petId}`. */
  path: string;
  operationId?: string;
  /** Alternatives, in document order; empty when no security applies. */
  security: SecurityRequirement[];
};

export type MiddlewareContext = { operation: OperationInfo };

/**
 * Wraps a request: change it, call `next` (zero, one or more times; use
 * `request.clone()` to send a body twice), and return a response.
 *
 * ```ts
 * const timing: Middleware = async (request, next) => {
 *   const start = performance.now();
 *   try {
 *     return await next(request);
 *   } finally {
 *     console.log(request.method, request.url, performance.now() - start);
 *   }
 * };
 * ```
 */
export type Middleware = (
  request: Request,
  next: (request: Request) => Promise<Response>,
  context: MiddlewareContext,
) => Promise<Response>;

/**
 * An operation in `paths`. `statuses` maps each response key to the statuses it
 * covers, as `[successes, failures]`; `default` also takes undeclared ones.
 */
type Operation = { request: object; responses: object; statuses: object };
type Methods<P> = { [K in keyof P]: keyof P[K] }[keyof P] & string;
type PathsWith<P, M> = { [K in keyof P]: M extends keyof P[K] ? K : never }[keyof P] & string;
type OperationAt<P, K extends keyof P, M> = P[K][M & keyof P[K]] extends infer O extends Operation
  ? O
  : never;
type Args<O extends Operation> = {} extends O["request"] & RequestOptions
  ? [init?: O["request"] & RequestOptions]
  : [init: O["request"] & RequestOptions];

export type Client<P> = {
  // `& {}` keeps the paths listed in the error for a path that does not exist.
  [M in Methods<P>]: <K extends PathsWith<P, M> & {}>(
    path: K,
    ...args: Args<OperationAt<P, K, M>>
  ) => Promise<ApiResponse<OperationAt<P, K, M>>>;
} & {
  /** A new client with `middleware` added inside this client's; this client is unchanged. */
  with(...middleware: Middleware[]): Client<P>;
};

/**
 * Serialization of a parameter where it differs from the default for its location:
 * `form` with `explode: true` for query and cookie, `simple` without explode for path and header.
 */
type ParamStyle = { style?: "deepObject" | "cookie" | "json"; explode?: boolean; reserved?: true };

/**
 * How a property of a form body is sent, where the document says: as a media type,
 * or like a query parameter. Other properties are sent by their value: objects as
 * JSON, arrays as one field per item, and the rest as text, or binary for a `Blob`.
 */
type Encoding = { contentType: string } | { style: ParamStyle };

type SecuritySchemeSpec =
  | { type: "apiKey"; in: "query" | "header" | "cookie"; name: string }
  | { type: "http"; /** Lowercased, e.g. `bearer`. */ scheme: string }
  | { type: "oauth2" | "openIdConnect" | "mutualTLS" };

/** Data from the document that the runtime needs; `operations` is keyed by `METHOD path`. */
type Spec = {
  baseUrl: string;
  /** Every HTTP method the document uses, as written: the client's callable properties. */
  methods: string[];
  security: SecurityRequirement[];
  securitySchemes: Record<string, SecuritySchemeSpec>;
  operations: Record<string, {
    operationId?: string;
    /** The operation's own server, used instead of `baseUrl`. */
    server?: string;
    /** Parameters with a non-default serialization, keyed by `location:name`. */
    styles?: Record<string, ParamStyle>;
    security?: SecurityRequirement[];
    /** The media type a body is sent as when the call names none. */
    contentType?: string;
    /** The media type of the `querystring` parameter. */
    querystring?: string;
    querystringEncoding?: Record<string, Encoding>;
    /** Form body encodings, keyed by media type and then by property. */
    encoding?: Record<string, Record<string, Encoding>>;
  }>;
};

type Init = RequestOptions & {
  path?: Record<string, unknown>;
  query?: Record<string, unknown>;
  header?: Record<string, unknown>;
  cookie?: Record<string, unknown>;
  querystring?: unknown;
  contentType?: string;
  body?: unknown;
};

function makeClient<P>(options: ClientOptions, spec: Spec): Client<P> {
  const send = async (method: string, path: string, init: Init = {}) => {
    const { path: pathParams, query, header, cookie, querystring, contentType, body, middleware, ...rest } = init;
    const op = `${method} ${path}`;
    // A leading "//" would make a relative URL point at another host in browsers.
    if (!path.startsWith("/") || path.startsWith("//")) {
      throw new TypeError(`${op}: the path must be a template from the document, starting with one "/"`);
    }
    const info = spec.operations[op] ?? {};
    const style = (location: string, name: string) => info.styles?.[`${location}:${name}`] ?? {};

    const base = options.baseUrl ?? info.server ?? spec.baseUrl;
    let url = base.replace(/\/+$/, "") + path.split("/").map((segment) => {
      if (!segment.includes("{")) return segment;
      const value = segment.replace(/\{([^}]+)\}/g, (_, name: string) =>
        simple(pathParams?.[name], style("path", name).explode ?? false, encodeURIComponent));
      // URL parsing would drop or climb out of these, sending the request elsewhere.
      if (/^(?:\.|%2e){0,2}$/i.test(value)) {
        throw new TypeError(`${op}: path segment "${segment}" must not be empty, "." or ".."`);
      }
      return value;
    }).join("/");
    const search = querystring !== undefined
      ? encodeQuerystring(querystring, info.querystring, info.querystringEncoding)
      : present(query).flatMap(([name, value]) => queryPairs(name, value, style("query", name))).join("&");
    if (search) url += `?${search}`;

    const headers = new Headers(
      typeof options.headers === "function" ? await options.headers() : options.headers,
    );
    for (const [name, value] of present(header)) {
      headers.set(name, simple(value, style("header", name).explode ?? false, String));
    }
    const cookies = present(cookie).flatMap(([name, value]) => {
      const s = style("cookie", name);
      // `cookie` style sends values as-is; `form` percent-encodes them.
      const enc = s.style === "cookie" ? String : encodeURIComponent;
      return form(name, value, s.explode ?? true, enc);
    });
    if (cookies.length) {
      headers.set("cookie", [headers.get("cookie"), ...cookies].filter(Boolean).join("; "));
    }
    new Headers(rest.headers).forEach((value, name) => {
      if (name !== "cookie") return headers.set(name, value);
      // The call's cookies join the ones set so far, replacing those of the same name.
      for (const pair of value.split(";")) {
        const [cookie = "", ...parts] = pair.split("=");
        if (cookie.trim()) setCookie(headers, cookie.trim(), parts.join("=").trim());
      }
    });

    const mediaType = contentType ?? info.contentType;
    const encoded = encodeBody(body, mediaType, headers, info.encoding?.[mediaType ?? ""]);
    const request = new Request(url, {
      ...rest,
      method,
      headers,
      body: encoded,
      // Required by Node to send a stream.
      ...(encoded instanceof ReadableStream ? { duplex: "half" } : {}),
    } as RequestInit);
    const context: MiddlewareContext = {
      operation: {
        method,
        path,
        ...(info.operationId ? { operationId: info.operationId } : {}),
        security: info.security ?? spec.security,
      },
    };
    const chain = [...(options.middleware ?? []), ...(middleware ?? [])];
    // `async`, so that a synchronous throw from a middleware or fetch rejects like any other failure.
    const dispatch = (i: number) => async (req: Request): Promise<Response> => {
      const mw = chain[i];
      return mw ? mw(req, dispatch(i + 1), context) : (options.fetch ?? fetch)(req);
    };
    const response = await dispatch(0)(request);
    return {
      status: response.status,
      ok: response.ok,
      data: await decodeBody(response, method),
      response,
    };
  };
  return new Proxy({}, {
    get: (target, method) => {
      if (method === "with") {
        return (...middleware: Middleware[]) =>
          makeClient<P>({ ...options, middleware: [...(options.middleware ?? []), ...middleware] }, spec);
      }
      // Only the document's methods send. Everything else (`then`, `toJSON`, inspection
      // hooks, lowercase names) resolves as on a plain object, so nothing sends by accident.
      return typeof method === "string" && spec.methods.includes(method)
        ? (path: string, init?: Init) => send(method, path, init)
        : Reflect.get(target, method);
    },
  }) as Client<P>;
}

/** Entries whose value is neither `undefined` nor `null`. */
function present(values: object | undefined): [string, unknown][] {
  return Object.entries(values ?? {}).filter(([, v]) => v !== undefined && v !== null);
}

type Encode = (s: string) => string;

/**
 * The whole query string from the value of a `querystring` parameter: form
 * fields, or else the value serialized as its media type and percent-encoded.
 */
function encodeQuerystring(
  value: unknown,
  mediaType = "application/x-www-form-urlencoded",
  encoding?: Record<string, Encoding>,
): string {
  switch (mediaKind(mediaType)) {
    case "form":
      return typeof value === "string" ? value.replace(/^\?/, "") : formEncode(value as Record<string, unknown>, encoding);
    case "json":
      return encodeURIComponent(JSON.stringify(value));
    default:
      return encodeURIComponent(String(value));
  }
}

/** RFC 6570 reserved expansion: reserved characters and %XX triples pass through. */
function encodeReserved(s: string): string {
  return s.replace(
    /%[0-9A-Fa-f]{2}|[^A-Za-z0-9\-._~:/?@!$'()*,;=]/gu,
    (c) => (c.length === 3 && c[0] === "%" ? c : encodeURIComponent(c)),
  );
}

/** `simple` style: `a,b` for arrays; `k,v` or, exploded, `k=v` for objects. */
function simple(value: unknown, explode: boolean, enc: Encode): string {
  if (Array.isArray(value)) return value.map((v) => enc(String(v))).join(",");
  if (value !== null && typeof value === "object") {
    return present(value)
      .map(([k, v]) => `${enc(k)}${explode ? "=" : ","}${enc(String(v))}`)
      .join(",");
  }
  return value === undefined ? "" : enc(String(value));
}

/** `form` style pairs; also used for cookies, which only differ in the delimiter. */
function form(name: string, value: unknown, explode: boolean, enc: Encode): string[] {
  if (Array.isArray(value)) {
    return explode
      ? value.map((v) => `${enc(name)}=${enc(String(v))}`)
      : [`${enc(name)}=${value.map((v) => enc(String(v))).join(",")}`];
  }
  if (typeof value === "object" && value !== null) {
    return explode
      ? present(value).map(([k, v]) => `${enc(k)}=${enc(String(v))}`)
      : [`${enc(name)}=${simple(value, false, enc)}`];
  }
  return [`${enc(name)}=${enc(String(value))}`];
}

function queryPairs(name: string, value: unknown, s: ParamStyle): string[] {
  const enc = s.reserved ? encodeReserved : encodeURIComponent;
  if (s.style === "json") return [`${enc(name)}=${enc(JSON.stringify(value))}`];
  if (s.style === "deepObject" && typeof value === "object" && value !== null) {
    return present(value).flatMap(([k, v]) =>
      (Array.isArray(v) ? v : [v]).map((item) => `${encodeURIComponent(`${name}[${k}]`)}=${enc(String(item))}`)
    );
  }
  return form(name, value, s.explode ?? true, enc);
}

/** `application/x-www-form-urlencoded` fields, encoded as {@link Encoding} says. */
function formEncode(value: Record<string, unknown>, encoding: Record<string, Encoding> = {}): string {
  return present(value).flatMap(([name, v]) => {
    const e = encoding[name];
    if (e && "style" in e) return queryPairs(name, v, e.style);
    const contentType = e?.contentType;
    return items(v).map((item) => {
      const json = contentType ? mediaKind(contentType) === "json" : typeof item === "object";
      return new URLSearchParams([[name, json ? JSON.stringify(item) : String(item)]]).toString();
    });
  }).join("&");
}

/**
 * `multipart/form-data` parts, encoded as {@link Encoding} says. A part with a
 * media type is sent as a `Blob`, as `FormData` has no other way to give it one.
 */
function formData(value: Record<string, unknown>, encoding: Record<string, Encoding> = {}): FormData {
  const data = new FormData();
  for (const [name, v] of present(value)) {
    const e = encoding[name];
    if (e && "style" in e) {
      // Like a query parameter, but without percent-encoding.
      for (const pair of queryPairs(name, v, e.style)) {
        const i = pair.indexOf("=");
        data.append(decodeURIComponent(pair.slice(0, i)), decodeURIComponent(pair.slice(i + 1)));
      }
      continue;
    }
    // A list or range of media types leaves the choice to the Blob's own type.
    const type = e && !/[,*]/.test(e.contentType) ? e.contentType : undefined;
    for (const item of items(v)) {
      if (item instanceof Blob) {
        if (item.type || !type) data.append(name, item);
        else data.append(name, new Blob([item], { type }), (item as Blob & { name?: string }).name);
      } else if (type ? mediaKind(type) === "json" : typeof item === "object") {
        data.append(name, new Blob([JSON.stringify(item)], { type: type ?? "application/json" }));
      } else if (type && essence(type) !== "text/plain") {
        data.append(name, new Blob([String(item)], { type }));
      } else {
        data.append(name, String(item));
      }
    }
  }
  return data;
}

/** The items of an array, or else the value itself, leaving out `undefined` and `null`. */
function items(value: unknown): unknown[] {
  return (Array.isArray(value) ? value : [value]).filter((v) => v !== undefined && v !== null);
}

type Kind = "json" | "form" | "multipart" | "text" | "sse" | "jsonl" | "json-seq" | "binary";

/** The media type without parameters, lowercased. */
function essence(mediaType: string): string {
  return (mediaType.split(";")[0] ?? "").trim().toLowerCase();
}

function mediaKind(mediaType: string): Kind {
  const mt = essence(mediaType);
  if (mt === "application/json" || mt.endsWith("+json")) return "json";
  if (mt === "application/x-www-form-urlencoded") return "form";
  if (mt === "text/event-stream") return "sse";
  if (mt === "application/jsonl" || mt === "application/x-ndjson") return "jsonl";
  if (mt === "application/json-seq" || mt.endsWith("+json-seq")) return "json-seq";
  if (mt.startsWith("multipart/")) return "multipart";
  if (mt.startsWith("text/")) return "text";
  return "binary";
}

function isRawBody(body: unknown): body is BodyInit {
  return typeof body === "string" || body instanceof Blob || body instanceof FormData ||
    body instanceof URLSearchParams || body instanceof ArrayBuffer ||
    ArrayBuffer.isView(body) || body instanceof ReadableStream;
}

function encodeBody(
  body: unknown,
  contentType: string | undefined,
  headers: Headers,
  encoding?: Record<string, Encoding>,
): BodyInit | null {
  if (body === undefined) return null;
  const kind = contentType ? mediaKind(contentType) : isRawBody(body) ? "binary" : "json";
  if (kind === "json") {
    headers.set("content-type", contentType ?? "application/json");
    return JSON.stringify(body);
  }
  if (kind === "form" && !isRawBody(body)) {
    headers.set("content-type", contentType!);
    return formEncode(body as Record<string, unknown>, encoding);
  }
  if (kind === "multipart") {
    // Only fetch knows the boundary, so it sets the header.
    headers.delete("content-type");
    return isRawBody(body) ? body : formData(body as Record<string, unknown>, encoding);
  }
  if (contentType) headers.set("content-type", contentType);
  return body as BodyInit;
}

/**
 * A response body that is not what its `Content-Type` says (an HTML error page
 * served as `application/json`, say). `response` is the response, its body
 * consumed; `text` is the body, or the offending item of a sequential media type.
 */
export class DecodeError extends Error {
  response: Response;
  text: string;
  constructor(response: Response, text: string, cause: unknown) {
    const reason = cause instanceof Error ? cause.message : String(cause);
    super(`${response.status} response body is not valid ${response.headers.get("content-type")}: ${reason}`, { cause });
    this.name = "DecodeError";
    this.response = response;
    this.text = text;
  }
}

/** `JSON.parse`, with a failure reported as a {@link DecodeError}. */
function parseJson(response: Response, text: string): unknown {
  try {
    return JSON.parse(text);
  } catch (cause) {
    throw new DecodeError(response, text, cause);
  }
}

async function decodeBody(response: Response, method: string): Promise<unknown> {
  if (method === "HEAD" || [204, 205, 304].includes(response.status)) return undefined;
  const contentType = response.headers.get("content-type");
  if (!contentType) return (await response.text()) || undefined;
  switch (mediaKind(contentType)) {
    case "json": {
      const text = await response.text();
      return text ? parseJson(response, text) : undefined;
    }
    case "jsonl":
      return mapItems(split(response, "\n"), (line) => line.trim() ? parseJson(response, line) : undefined);
    case "json-seq":
      return mapItems(split(response, "\x1e"), (rec) => rec.trim() ? parseJson(response, rec) : undefined);
    case "sse":
      return events(response);
    case "text":
    case "form":
      return response.text();
    default:
      return response.blob();
  }
}

async function* split(response: Response, separator: string): AsyncGenerator<string> {
  if (!response.body) return;
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
  let done = false;
  let buffer = "";
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      buffer += chunk.value;
      let i: number;
      while ((i = buffer.indexOf(separator)) >= 0) {
        yield buffer.slice(0, i);
        buffer = buffer.slice(i + separator.length);
      }
    }
    done = true;
    if (buffer) yield buffer;
  } finally {
    // Stop the download when the consumer breaks out early.
    if (!done) await reader.cancel().catch(() => {});
  }
}

async function* mapItems<T>(items: AsyncIterable<string>, parse: (item: string) => T | undefined): AsyncGenerator<T> {
  for await (const item of items) {
    const value = parse(item);
    if (value !== undefined) yield value;
  }
}

/**
 * One event. `id` and `retry` are the last ones the stream set, possibly in an
 * earlier block: the last event ID and the reconnection time.
 */
export type ServerSentEvent = { data: string; event?: string; id?: string; retry?: number };

async function* events(response: Response): AsyncGenerator<ServerSentEvent> {
  const state: { id?: string; retry?: number } = {};
  let event: string | undefined;
  let data: string[] = [];
  for await (let line of split(response, "\n")) {
    if (line.endsWith("\r")) line = line.slice(0, -1);
    if (line === "") {
      if (data.length) yield { ...state, ...(event === undefined ? {} : { event }), data: data.join("\n") };
      event = undefined;
      data = [];
      continue;
    }
    if (line.startsWith(":")) continue;
    const colon = line.indexOf(":");
    const field = colon < 0 ? line : line.slice(0, colon);
    let value = colon < 0 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "data") data.push(value);
    else if (field === "event") event = value;
    else if (field === "id") state.id = value;
    else if (field === "retry" && /^\d+$/.test(value)) state.retry = Number(value);
  }
}

/** A credential, or a function that returns one (`undefined` or `""` when unavailable). */
export type Credential =
  | string
  | ((scopes: string[], context: MiddlewareContext) => string | undefined | Promise<string | undefined>);

/**
 * Middleware that authenticates each request as the document's `security` says:
 * it picks the first alternative whose credentials are all available and places
 * each credential where its security scheme expects it. A credential given as
 * `undefined`, such as an unset environment variable, is unavailable. Alternatives that add
 * nothing to the request (anonymous `{}`, `mutualTLS` alone) are taken only when
 * no other is; with none, the request goes out unauthenticated.
 *
 * - `http` bearer, `oauth2`, `openIdConnect`: `Authorization: Bearer <credential>`
 * - `http` basic: the credential is `user:password`, sent base64-encoded
 * - other `http` schemes: `Authorization: <scheme> <credential>`
 * - `apiKey`: the credential as the named header, or percent-encoded as the named
 *   query parameter or cookie
 * - `mutualTLS`: nothing to add; configure TLS on the fetch implementation
 *
 * ```ts
 * createClient({ middleware: [auth({ bearer: (scopes) => getToken(scopes) })] });
 * ```
 */
export function auth(credentials: { [K in SecuritySchemeName]?: Credential | undefined }): Middleware {
  return async (request, next, context) => {
    // Alternatives that resolve to no credentials are skipped: sending nothing is
    // also what happens when none is satisfiable.
    for (const requirement of context.operation.security) {
      const resolved = await satisfy(requirement, credentials, context);
      if (resolved?.length) {
        for (const [scheme, value] of resolved) request = await authenticate(request, scheme, value);
        break;
      }
    }
    return next(request);
  };
}

/** The credential for every scheme of a requirement, or `undefined` if one is missing. */
async function satisfy(
  requirement: SecurityRequirement,
  credentials: { [K in SecuritySchemeName]?: Credential | undefined },
  context: MiddlewareContext,
): Promise<[SecuritySchemeSpec, string][] | undefined> {
  const out: [SecuritySchemeSpec, string][] = [];
  for (const [name, scopes] of Object.entries(requirement) as [SecuritySchemeName, string[]][]) {
    const scheme = spec.securitySchemes[name];
    if (!scheme) return undefined;
    if (scheme.type === "mutualTLS") continue;
    // An assertion, not an annotation: with no schemes, `credentials[name]` is `never`.
    const credential = credentials[name] as Credential | undefined;
    const value = typeof credential === "function" ? await credential(scopes, context) : credential;
    if (value === undefined || value === "") return undefined;
    out.push([scheme, value]);
  }
  return out;
}

async function authenticate(request: Request, scheme: SecuritySchemeSpec, value: string): Promise<Request> {
  switch (scheme.type) {
    case "apiKey":
      if (scheme.in === "query") {
        return withQuery(request, `${encodeURIComponent(scheme.name)}=${encodeURIComponent(value)}`);
      }
      if (scheme.in === "cookie") setCookie(request.headers, scheme.name, encodeURIComponent(value));
      else request.headers.set(scheme.name, value);
      return request;
    case "http": {
      if (scheme.scheme === "basic") {
        const bytes = new TextEncoder().encode(value);
        request.headers.set("authorization", `Basic ${btoa(String.fromCharCode(...bytes))}`);
      } else {
        request.headers.set("authorization", `${scheme.scheme === "bearer" ? "Bearer" : scheme.scheme} ${value}`);
      }
      return request;
    }
    case "oauth2":
    case "openIdConnect":
      request.headers.set("authorization", `Bearer ${value}`);
      return request;
    default:
      return request;
  }
}

/** `request` with `pair` appended to its query; the rest of the URL stays as it is. */
async function withQuery(request: Request, pair: string): Promise<Request> {
  const hash = request.url.indexOf("#");
  const [url, fragment] = hash < 0 ? [request.url, ""] : [request.url.slice(0, hash), request.url.slice(hash)];
  const joined = `${url}${url.includes("?") ? "&" : "?"}${pair}${fragment}`;
  if (request.body === null) return new Request(joined, request);
  // `new Request(joined, request)` would pass the body on as a stream, which Chromium
  // and WebKit refuse and Firefox sends empty: read it first.
  return new Request(joined, {
    method: request.method,
    headers: request.headers,
    body: await request.arrayBuffer(),
    mode: request.mode,
    credentials: request.credentials,
    cache: request.cache,
    redirect: request.redirect,
    referrer: request.referrer,
    referrerPolicy: request.referrerPolicy,
    integrity: request.integrity,
    keepalive: request.keepalive,
    signal: request.signal,
  });
}

/** Sets cookie `name` in the Cookie header, replacing an existing cookie of that name. */
function setCookie(headers: Headers, name: string, value: string): void {
  const others = (headers.get("cookie") ?? "")
    .split(";")
    .map((c) => c.trim())
    .filter((c) => c && !c.startsWith(`${name}=`));
  headers.set("cookie", [...others, `${name}=${value}`].join("; "));
}
