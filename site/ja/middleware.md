# ミドルウェア

ミドルウェアはリクエストをラップし、レスポンスを返します。ミドルウェアの中では、リクエストを変更したり、`next` を 0 回以上呼び出したりできます。

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

- 最初のミドルウェアが最も外側になります。1 回の呼び出しに渡したミドルウェア（`api.GET("/pets", { middleware: [...] })`）は、クライアントのミドルウェアの内側で実行されます。
- `next` はリクエストを次のミドルウェアに渡し、最後のミドルウェアでは `fetch` に渡します。ミドルウェアが `next` を呼び出さずに `Response` を返すと、リクエストはネットワーク経由で送信されません。この仕組みはキャッシュやモックに役立ちます。
- `fetch` 自体が投げるエラーは、`next` から投げられます。
- レスポンス本体の読み込みとデコードは、チェーンが終了した後に行われます。そのため、ミドルウェアではレスポンスを調整でき、本体の読み込み中に発生したエラーは `DecodeError` も含めて `next` を経由しません。

## 操作の情報 {#the-operation}

第 3 引数は `{ operation }` で、OpenAPI ドキュメントに記述された操作を表します。

| フィールド | 内容 |
| --- | --- |
| `method` | 呼び出しで使われたとおりの HTTP メソッド |
| `path` | `/pets/{petId}` などのパステンプレート |
| `operationId` | `operationId`（ドキュメントに記述があるとき） |
| `security` | 適用されるセキュリティ要件（ドキュメントでの記述順） |

## `with` によるクライアント単位のミドルウェア {#per-client-middleware-with-with}

`api.with(...middleware)` は、渡したミドルウェアを元のクライアントのミドルウェアの内側で実行する、新しいクライアントを返します。なお、`api` 自体は変更されません。

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

共有しているクライアントを変更する代わりに `with` を使うと、サーバー上で扱うテナントやユーザーのトークンなど、リクエストごとの状態がほかのリクエストに漏れるのを防げます。
