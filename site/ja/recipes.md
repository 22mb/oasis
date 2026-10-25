# レシピ

どのレシピも、生成されるクライアントがすでに提供している機能だけを使います。

## 想定外のステータスで例外を投げる {#throwing-on-unexpected-statuses}

呼び出しは、2xx 以外のレスポンスに対して例外を投げずにレスポンスを返します。データ取得ライブラリーの中など、例外のほうが適しているときは、呼び出しをラップして、想定するステータスを渡します。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
class HttpError extends Error {
  response: Response;
  data: unknown;
  constructor(response: Response, data: unknown) {
    super(`${response.status} ${response.statusText}`);
    this.name = "HttpError";
    this.response = response;
    this.data = data;
  }
}

type Result = {
  status: number;
  data: unknown;
  response: Response;
};

/**
 * ステータスが `status` のレスポンスのデータで解決し、
 * それ以外のステータスでは HttpError を投げる。
 */
async function expectStatus<R extends Result, S extends R["status"]>(
  call: Promise<R>,
  status: S,
): Promise<Extract<R, { status: S }>["data"]> {
  const res = await call;
  if (res.status !== status) {
    throw new HttpError(res.response, res.data);
  }
  return res.data as Extract<R, { status: S }>["data"];
}

const call = api.GET("/pets/{petId}", { path: { petId: 1 } });
// `pet` の型は 200 のレスポンスから決まる。
const pet = await expectStatus(call, 200);
```

## TanStack Query {#tanstack-query}

生成されたフックは必要ありません。`queryFn` の中で、TanStack Query が提供する `signal` を渡して呼び出してください。そうすれば、クエリーがキャンセルされたときにリクエストが中止されます。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
class HttpError extends Error {
  response: Response;
  data: unknown;
  constructor(response: Response, data: unknown) {
    super(String(response.status));
    this.response = response;
    this.data = data;
  }
}
async function expectStatus<R extends { status: number; data: unknown; response: Response }, S extends R["status"]>(
  call: Promise<R>,
  status: S,
): Promise<Extract<R, { status: S }>["data"]> {
  const res = await call;
  if (res.status !== status) throw new HttpError(res.response, res.data);
  return res.data as Extract<R, { status: S }>["data"];
}
// ---cut---
import { useQuery } from "@tanstack/react-query";

function usePet(petId: number) {
  return useQuery({
    queryKey: ["pets", petId],
    queryFn: ({ signal }) => {
      const call = api.GET("/pets/{petId}", {
        path: { petId },
        signal,
      });
      return expectStatus(call, 200);
    },
  });
}
```

## 環境ごとにベース URL を設定する {#setting-the-base-url-for-each-environment}

ドキュメントのサーバーが開発用のホストを指しているときなど、環境ごとに異なるサーバーを呼び出すには、ビルドの環境変数から `baseUrl` を設定します。Vite では次のようにします。

```ts twoslash
import { createClient } from "./api.ts";
declare global {
  interface ImportMeta {
    env: { VITE_API_BASE_URL: string };
  }
}
// ---cut---
const api = createClient({ baseUrl: import.meta.env.VITE_API_BASE_URL });
```

ブラウザーでは、Vite の開発サーバーの `server.proxy` オプションと組み合わせるなどして、`"/api"` のような相対的な `baseUrl` も使えます。

## 401 を受け取ったらトークンを更新する {#refreshing-a-token-on-401}

ミドルウェアは `next` を複数回呼び出せます。`401` のレスポンスを受け取った後に新しいトークンでリクエストを再試行するには、まだ送信していないリクエストのコピーを保持しておきます。

```ts twoslash
import { createClient, type Middleware } from "./api.ts";
// ---cut---
declare function signIn(): Promise<string>;

let token = await signIn();

const bearer: Middleware = async (request, next) => {
  // リクエスト本体は 1 回しか送信できないため、再試行用にコピーを保持しておく。
  const retry = request.clone();
  request.headers.set("authorization", `Bearer ${token}`);
  const response = await next(request);
  if (response.status !== 401) return response;
  token = await signIn();
  retry.headers.set("authorization", `Bearer ${token}`);
  return next(retry);
};

const api = createClient({ middleware: [bearer] });
```

## テストでレスポンスをモックする {#mocking-responses-in-tests}

`fetch` オプションを使うと、クライアント全体でネットワークへのアクセスが置き換えられます。

```ts twoslash
import { createClient } from "./api.ts";
// ---cut---
const pet = { id: 1, name: "Tama", kind: "cat" };

const api = createClient({
  baseUrl: "https://api.test",
  fetch: async () => Response.json(pet),
});
```

一部の操作だけをモックし、それ以外のリクエストはネットワーク経由で送信するには、`next` を呼び出さずに `Response` を返すミドルウェアを使います。

```ts twoslash
import { createClient, type Middleware } from "./api.ts";
const api = createClient();
// ---cut---
const mock: Middleware = async (request, next, { operation }) => {
  if (operation.method === "GET" && operation.path === "/pets") {
    return Response.json([{ id: 1, name: "Tama", kind: "cat" }]);
  }
  return next(request);
};

const mocked = api.with(mock);
```

## ストリーミングされるレスポンスを読み込む {#reading-streamed-responses}

JSON Lines やサーバー送信イベントのような逐次的なメディアタイプ（sequential media types）のレスポンスでは、`data` は非同期反復可能オブジェクトです。先に `status` で絞り込んでから、反復処理します。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
const res = await api.GET("/stream/jsonl");
if (res.status === 200) {
  for await (const pet of res.data) {
    console.log(pet.name);
  }
}
```

ループを抜けると、ダウンロードは停止します。また、項目を解析できないときは、ループが `DecodeError` を投げます。

## サーバーでリクエストごとにクライアントを派生させる {#per-request-clients-on-a-server}

サーバーでは、1 つのクライアントで多くのユーザーからのリクエストを処理できます。クライアントは一度だけ作成し、受け取ったリクエストごとに `with` でクライアントを派生させてください。そうすれば、あるユーザーのトークンが別のユーザーのリクエストに渡ることはありません。

```ts twoslash
import { createClient } from "./api.ts";
// ---cut---
const base = createClient({ baseUrl: "https://api.example.com" });

async function handle(incoming: Request): Promise<Response> {
  const token = incoming.headers.get("authorization");
  const api = base.with(async (request, next) => {
    if (token) request.headers.set("authorization", token);
    return next(request);
  });
  const res = await api.GET("/pets");
  return Response.json(res.status === 200 ? res.data : []);
}
```

## 生成されるファイルをプロジェクトに置く {#keeping-the-generated-file-in-your-project}

生成されるファイルをコミットすると、oasis-gen を実行しなくてもプロジェクトの型チェックができます。一方、ビルドの工程で生成すると、ファイルはドキュメントと同期した状態に保たれます。どちらの方法でも、生成されるファイルはプロジェクトのスタイル規則に従っていないため、リンターとフォーマッターの対象から除外してください。たとえば ESLint のフラット設定（flat config）では、次のようにします。

```js
import { globalIgnores } from "eslint/config";

export default [
  globalIgnores(["src/api.ts"]),
  // ...プロジェクトの設定
];
```

## よくある問題 {#common-problems}

| 問題 | 原因 | 解決方法 |
| --- | --- | --- |
| ブラウザー以外での `TypeError: Failed to parse URL` | ドキュメントにサーバーがないか `baseUrl` が相対的なため、リクエストの URL が相対 URL | 絶対 URL の `baseUrl` を設定 |
| `TS5097: An import path can only end with a '.ts' extension ...` | プロジェクトでインポートの `.ts` 拡張子が不許可 | `allowImportingTsExtensions` を有効にするか、`./api.js` または `./api` をインポート |
| `DecodeError` での `TS2554: Expected 0-1 arguments, but got 2.` | ES2022 より古い `lib` | `lib` を ES2022 以降に設定 |
| `TS2304: Cannot find name 'RequestInit'`、`'Blob'`、または `'FormData'` | `lib` の `dom` と `@types/node` のどちらも利用不可 | どちらかを追加 |
| エラーレスポンスでの `DecodeError` | サーバーから返された、`Content-Type` と一致しない本体（HTML のエラーページなど） | `DecodeError` を捕捉して `error.response.status` を確認。[エラー](./client#decodeerror)を参照 |
| ブラウザーで送信されないクッキーパラメーターやクッキーの API キー | `fetch` で設定された `Cookie` ヘッダーをブラウザーが無視 | ブラウザー自身によるクッキー送信を利用（`credentials` の fetch オプションなど） |
| 拒否される `nullable` | `nullable` が存在しない OpenAPI 3.1 か 3.2 のドキュメント | `type: [..., "null"]` を使用 |
