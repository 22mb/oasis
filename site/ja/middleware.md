# ミドルウェア

ミドルウェアはリクエストをラップし、レスポンスを返します。ミドルウェア内では、リクエストを変更したり、`next` を 0 回以上呼び出したりできます。

ミドルウェア内では、`next` は `fetch` と同じように動作し、`Request` を受け取って `Promise<Response>` を返します。リクエストがネットワークに届く前にほかのミドルウェアを経由するかどうかは、呼び出す側からは分かりません。

```ts twoslash
import { createClient, type Middleware } from "./api.ts";

const timing: Middleware = async (request, next, { operation }) => {
  const start = performance.now();
  try {
    return await next(request);
  } finally {
    const name = operation.operationId ?? operation.path;
    console.log(name, performance.now() - start);
  }
};

const retry: Middleware = async (request, next) => {
  for (let attempt = 0; ; attempt++) {
    // リクエスト本体は 1 回しか送信できないため、コピーを送信する。
    const response = await next(request.clone());
    if (response.status < 500 || attempt === 2) return response;
  }
};

const api = createClient({ middleware: [timing, retry] });
```

## ミドルウェアチェーンの動作 {#how-the-chain-runs}

<figure class="diagram">
<div class="drawing" role="region" aria-label="ミドルウェアのチェーンの図" tabindex="0">
<svg viewBox="-2 0 756 272" role="img" aria-label="ミドルウェアのチェーン" aria-describedby="mw-desc" fill="currentColor" font-size="13">
<desc id="mw-desc">リクエストは、クライアントのミドルウェア（timing → retry の順）と呼び出しごとのミドルウェア（cache）を経由して fetch に渡されます。レスポンスは逆の順序で返されます。cache は fetch を呼び出さずにレスポンスを返せます。ミドルウェアは 1 回の呼び出しでリクエストとレスポンスの両方を扱い、next より前のコードがリクエストの送信前に、next の後のコードがレスポンスの受信後に実行されます。チェーンの完了後、レスポンス本体がデコードされて結果になります。</desc>
<defs>
<marker id="mw-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z"/></marker>
<marker id="mw-arrow-accent" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" class="accent"/></marker>
</defs>
<path d="M240 50V44H470V50" fill="none" stroke="currentColor" stroke-width="1"/>
<text x="355" y="34" text-anchor="middle">クライアントのミドルウェア</text>
<path d="M520 50V44H610V50" fill="none" stroke="currentColor" stroke-width="1"/>
<text x="565" y="34" text-anchor="middle">呼び出しのミドルウェア</text>
<rect x="0" y="60" width="110" height="84" rx="8" class="box"/>
<text x="55" y="107" text-anchor="middle" class="mono" font-size="14">api.GET()</text>
<rect x="240" y="60" width="90" height="84" rx="8" class="box"/>
<text x="285" y="107" text-anchor="middle" class="mono" font-size="14">timing</text>
<rect x="380" y="60" width="90" height="84" rx="8" class="box"/>
<text x="425" y="107" text-anchor="middle" class="mono" font-size="14">retry</text>
<rect x="520" y="60" width="90" height="84" rx="8" class="box"/>
<text x="565" y="107" text-anchor="middle" class="mono" font-size="14">cache</text>
<rect x="660" y="60" width="90" height="84" rx="8" class="box"/>
<text x="705" y="100" text-anchor="middle" class="mono" font-size="14">fetch</text>
<text x="705" y="120" text-anchor="middle" font-size="12" class="muted">ネットワーク</text>
<line x1="110" y1="80" x2="238" y2="80" marker-end="url(#mw-arrow)"/>
<line x1="330" y1="80" x2="378" y2="80" marker-end="url(#mw-arrow)"/>
<line x1="470" y1="80" x2="518" y2="80" marker-end="url(#mw-arrow)"/>
<line x1="610" y1="80" x2="658" y2="80" marker-end="url(#mw-arrow)"/>
<line x1="660" y1="124" x2="612" y2="124" marker-end="url(#mw-arrow)"/>
<line x1="520" y1="124" x2="472" y2="124" marker-end="url(#mw-arrow)"/>
<line x1="380" y1="124" x2="332" y2="124" marker-end="url(#mw-arrow)"/>
<line x1="240" y1="124" x2="112" y2="124" marker-end="url(#mw-arrow)"/>
<text x="175" y="72" text-anchor="middle" font-size="12" class="mono muted">next(request)</text>
<text x="175" y="141" text-anchor="middle" font-size="12" class="muted">Response を返す</text>
<path d="M610 92C644 92 644 112 613 112" fill="none" stroke-width="2" stroke-dasharray="5 4" class="accent-stroke" marker-end="url(#mw-arrow-accent)"/>
<text x="285" y="164" text-anchor="middle" font-size="12" class="muted">最も外側</text>
<text x="425" y="164" text-anchor="middle" font-size="12" class="muted">next を再び呼べる</text>
<text x="565" y="164" text-anchor="middle" font-size="12" class="accent">途中で返せる</text>
<line x1="55" y1="144" x2="55" y2="198" marker-end="url(#mw-arrow)"/>
<rect x="0" y="200" width="110" height="44" rx="8" class="box"/>
<text x="55" y="227" text-anchor="middle">本体のデコード</text>
<line x1="110" y1="222" x2="238" y2="222" marker-end="url(#mw-arrow)"/>
<rect x="240" y="200" width="280" height="44" rx="8" class="box"/>
<text x="380" y="227" text-anchor="middle" class="mono" font-size="14">{ ok, status, data, response }</text>
<text x="0" y="266" font-size="12" class="muted">DecodeError はここで発生</text>
</svg>
</div>
<figcaption><code>api.GET()</code> のリクエストが、<code>createClient</code> の <code>[timing, retry]</code> と呼び出しごとの <code>[cache]</code> を経由する順序。ミドルウェアは 1 回の呼び出しでリクエストとレスポンスの両方を扱い、<code>await next(request)</code> より前のコードがリクエストの送信前に、後のコードがレスポンスの受信後に実行されます。強調されている経路は、<code>cache</code> が <code>next</code> を呼び出さずに <code>Response</code> を返す場合を示しています。</figcaption>
</figure>

- 最初のミドルウェアが最も外側になります。個別の呼び出しに渡したミドルウェア（`api.GET("/pets", { middleware: [...] })`）は、クライアントのミドルウェアの内側で実行されます。
- `next` はリクエストを次のミドルウェアに渡し、最後のミドルウェアでは `fetch` に渡します。ミドルウェアが `next` を呼び出さずに `Response` を返すと、リクエストはネットワーク経由で送信されません。この仕組みはキャッシュやモックに役立ちます。
- `fetch` 自体が投げるエラーは、`next` から投げられます。
- レスポンス本体の読み取りとデコードは、ミドルウェアチェーンが終了した後に行われます。そのため、ミドルウェアではレスポンスを調整でき、レスポンス本体の読み取り中に発生したエラーは `DecodeError` も含めて `next` から投げられません。

## 操作の情報 {#the-operation}

第 3 引数の `{ operation }` には、OpenAPI ドキュメントに記述された操作の情報が格納されています。

| フィールド | 内容 |
| --- | --- |
| `method` | 呼び出しで使われたとおりの HTTP メソッド |
| `path` | `/pets/{petId}` などのパステンプレート |
| `operationId` | `operationId`（ドキュメントに記述があるとき） |
| `security` | 適用されるセキュリティ要件（ドキュメントでの記述順） |

## `with` によるクライアント単位のミドルウェア {#per-client-middleware-with-with}

`api.with(...middleware)` は、渡したミドルウェアを元のクライアントのミドルウェアの内側で実行する新しいクライアントを返します。なお、`api` 自体は変更されません。

```ts twoslash
import { createClient, type Middleware } from "./api.ts";
const api = createClient();
// ---cut---
const tenant = (id: string): Middleware => (request, next) => {
  request.headers.set("x-tenant-id", id);
  return next(request);
};

const acme = api.with(tenant("acme"));
await acme.GET("/pets");
```

共有しているクライアントを直接変更する代わりに `with` を使うと、サーバー上で扱うテナントやユーザーのトークンなど、リクエストごとの状態がほかのリクエストに漏れるのを防げます。
