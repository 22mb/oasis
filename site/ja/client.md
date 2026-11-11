# 操作の呼び出し

生成されるファイルは `createClient` をエクスポートします。`createClient` が返すクライアントは、OpenAPI ドキュメントにある HTTP メソッドごとに 1 つずつメソッドを持ちます。

## クライアントの作成 {#creating-a-client}

```ts twoslash
import { createClient } from "./api.ts";

const api = createClient({
  baseUrl: "https://api.example.com/v1",
  headers: { "x-request-source": "web" },
});
```

| オプション | 説明 |
| --- | --- |
| `baseUrl` | ドキュメントのすべてのサーバーを置き換えるベース URL |
| `headers` | すべてのリクエストで送信するヘッダー。リクエストごとに呼び出される関数（非同期関数も可）も指定可能 |
| `middleware` | すべてのリクエストをラップするミドルウェア。配列の先頭が最も外側。[ミドルウェア](./middleware)を参照 |
| `fetch` | グローバルな `fetch` の代わりに、最終的な `Request` を引数として呼び出される関数 |

オプションはすべて省略可能です。資格情報を送信する方法については、[認証](./authentication)をご覧ください。

### サーバーと `baseUrl` {#servers-and-baseurl}

`baseUrl` を省略すると、各操作のリクエストはその操作のサーバーに送信されます。送信先は操作の `servers` の最初の項目であり、操作の `servers` がなければパスアイテムの `servers` の最初の項目、それもなければ最上位（ルート）の `servers` の最初の項目が使われます。サーバー変数には既定値が設定されます。

- ドキュメントにサーバーが 1 つもないときは、パスは現在のページからの相対パスのままになり、ブラウザーではそのまま動作します。ただし、それ以外の環境では `new Request` が相対 URL を拒否するため、`baseUrl` を設定してください。
- `"/api"` のような相対パスの `baseUrl` も、ブラウザーでしか動作しません。

環境ごとに異なるサーバーを使う方法については、[レシピ](./recipes#setting-the-base-url-for-each-environment)をご覧ください。

## 操作の呼び出し方 {#calling-an-operation}

操作は、HTTP メソッドを HTTP の表記どおりに、パステンプレートをドキュメントの記述どおりに指定して選択します。第 2 引数には、入力をリクエスト内の設定先ごとに分けて渡します。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
await api.GET("/pets/{petId}", { path: { petId: 1 } });

await api.GET("/pets", {
  query: { limit: 10, tags: ["a", "b"] },
  header: { "X-Request-Id": "r1" },
  cookie: { session: "s1" },
  // タイムアウトなど、その他の fetch のオプション:
  signal: AbortSignal.timeout(5000),
});

await api.POST("/pets", { body: { name: "Tama", kind: "cat" } });
```

| フィールド | 内容 |
| --- | --- |
| `path`、`query`、`header`、`cookie` | パラメーター。それぞれの `style` と `explode` の指定に従ってシリアライズ |
| `querystring` | `in: querystring` のパラメーター（[後述](#querystring-parameters)）。`query` の代わりに使用 |
| `body`、`contentType` | リクエスト本体とそのメディアタイプ。[リクエスト本体](#request-bodies)を参照 |
| その他 | `new Request` に渡すオプション（`method` を除く）。Next.js の `next` など、`Request` が保持しないオプションは `fetch` に届かない |

第 2 引数を省略できるのは、操作に必須の入力がないときだけです。

ヘッダーは次の順に適用され、後に適用されたヘッダーが先のヘッダーを上書きします。

1. クライアントの `headers`
2. `header` パラメーター
3. `cookie` パラメーター（すでに設定されている `Cookie` ヘッダーへの追加）
4. 呼び出し自体の `headers` オプション（`Cookie` ヘッダーはそれまでに設定された cookie に結合され、同名の cookie のみを上書き）

ブラウザーは `fetch` で設定された `Cookie` ヘッダーを無視するため、`cookie` パラメーターは Node.js と Bun でのみ送信されます。ブラウザーでは、代わりにブラウザー自身が保持する cookie が送信されます。

### リクエスト本体 {#request-bodies}

`body` の型は、リクエストのメディアタイプによって決まります。

| メディアタイプ | `body` |
| --- | --- |
| `application/json`、`*+json` | スキーマの型 |
| `application/x-www-form-urlencoded` | スキーマの型（フォームエンコードして送信） |
| `multipart/form-data` | スキーマの型（バイナリーのパートは `Blob`）。スキーマがないときは `FormData` |
| その他の `multipart/*` | `Blob`（`type` にメディアタイプと `boundary` パラメーターを指定） |
| `text/*` | `string` |
| `*/*`、`application/*` | スキーマの型、または `unknown` |
| その他 | `FormData` と `URLSearchParams` を除く `BodyInit` |

`contentType` でメディアタイプを選択すると、`body` の型はそのメディアタイプに従います。操作が宣言しているメディアタイプが 1 つだけで、それが範囲でないときは、`contentType` を省略できます。また、複数のメディアタイプを宣言しているときも、`contentType` を省略すると、その操作の最初の `application/json` メディアタイプが使われます。どちらの場合も、リクエスト本体は `charset` などのパラメーターを含め、宣言されたとおりのメディアタイプで送信されます。範囲を表すメディアタイプでは、`contentType` に範囲内のメディアタイプを指定します。`image/*` なら `` `image/${string}` ``、`*/*` なら任意の文字列です。`contentType` は必須ですが、`*/*` に限り、操作に `application/json` のメディアタイプがないときは省略できます。省略すると、リクエスト本体は JSON として送信されます。ただし、文字列や `Blob`、`FormData` などの `BodyInit` はそのまま送信されます。なお、リクエスト本体が省略可能なときは、`body` と `contentType` の両方を省略できます。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
await api.POST("/pets", {
  contentType: "application/x-www-form-urlencoded",
  body: { name: "Tama", kind: "cat" },
});
```

JSON の本体は `JSON.stringify` でシリアライズされるため、メディアタイプが JSON のときは、文字列の本体も JSON 文字列として送信されます。`Content-Type` ヘッダーは本体のメディアタイプに基づいて設定されるため、`headers` に渡すオブジェクトでは指定できません。マルチパートの本体では、`headers` で設定した `Content-Type` ヘッダーに代わり、`fetch` が境界（boundary）を含めて `Content-Type` ヘッダーを設定します。また、リクエスト本体には、そのスキーマの[リクエスト用の型](./strict-types#requests-and-responses-have-separate-types)が使われます。

### フォームの本体 {#form-bodies}

フォームの本体では、オブジェクトの各プロパティが 1 つのフィールド（`application/x-www-form-urlencoded`）またはパート（`multipart/form-data`）として送信されます。配列は要素ごとに、プロパティ名を名前とするフィールドまたはパートになり、`undefined` と `null` の要素は送信されません。各値は、ドキュメントがそのプロパティに指定した Encoding Object に従って送信され、Encoding Object がないときは値の型に応じて次のように送信されます。

| 値 | フォームのフィールド | マルチパートのパート |
| --- | --- | --- |
| オブジェクト、または配列内の配列 | JSON | JSON（`application/json`） |
| `Blob` | 未対応 | バイナリー。メディアタイプは `Blob` 自身の型（型がないときは `application/octet-stream`） |
| その他 | `String(value)` | テキスト |

OpenAPI ではバイナリーデータを `type` のないスキーマで表すため、マルチパートの本体では、スキーマに `type` がないプロパティは `Blob` になります。OpenAPI 3.0 では、`type: string, format: binary` が同じ意味になります。一方、`type: string` のパートは、`contentMediaType` が指定されていてもテキストとして扱われます。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
// ---cut---
await api.POST("/pets/{petId}/photo", {
  path: { petId: 1 },
  body: { file: new File([png], "tama.png", { type: "image/png" }), caption: "Tama" },
});
```

Encoding Object を指定すると、プロパティの送信方法が次のように変わります。

- `contentType` はメディアタイプを指定します。メディアタイプが JSON のときは、フォームのフィールドでも値が JSON として送信されます。マルチパートのパートはその `Content-Type` で送信され、型のない `Blob` にもこのメディアタイプが設定されます。`image/png, image/jpeg` のように複数のメディアタイプが列挙されているときは、`Blob` 自身の型によって使用するメディアタイプが決まります。
- `style`、`explode`、`allowReserved` を指定すると、プロパティはクエリーパラメーターと同じ形式で送信されます。マルチパートの本体では、名前と値の組がそれぞれパートになり、パーセントエンコードは行われません。

`FormData` は、JSON や `Blob` のようにメディアタイプを持つパートにファイル名を付与して送信します。そのため、FastAPI の `UploadFile` や multer など、ファイル名のあるパートをすべてファイルとして扱うサーバーでは、JSON のパートもファイルとして読み込まれます。

### `querystring` パラメーター {#querystring-parameters}

`in: querystring` のパラメーターは、クエリー文字列全体を表します。このパラメーターの型とエンコード方法は、メディアタイプによって決まります。

| メディアタイプ | 値 | 送信形式 |
| --- | --- | --- |
| `application/x-www-form-urlencoded` | スキーマの型。スキーマがないときは `Record<string, unknown>` | フォームエンコード |
| `application/json`、`*+json` | スキーマの型 | `encodeURIComponent(JSON.stringify(value))` |
| その他 | `string` | `encodeURIComponent(String(value))` |

## レスポンス {#responses}

すべての呼び出しは、`{ ok, status, data, response }` で解決するプロミスを返します。`status` を判定すると、`data` は一致するレスポンスの型に絞り込まれます。また、`ok` は `response.ok` と同じ値です。型付けの規則については、[厳密な型](./strict-types#responses-are-typed-by-status)をご覧ください。なお、クライアントは `response` の本体を読み取って `data` を作ります（逐次的なメディアタイプでは、反復処理の間に読み取ります）。そのため、本体は `response` からではなく `data` から読み取ってください。

### レスポンス本体 {#response-bodies}

クライアントは、レスポンスの実際の `Content-Type` に基づいてレスポンス本体をデコードします。一方、`data` の型は、ドキュメントで宣言されているメディアタイプから決まります。

| メディアタイプ | `data` |
| --- | --- |
| `application/json`、`*+json` | JSON として解析した値。本体が空のときは `undefined` |
| `text/*`、`application/x-www-form-urlencoded` | `string`。スキーマに `type` がない `text/*` は `Blob` |
| `application/jsonl`、`application/x-ndjson`、`application/json-seq`、`*+json-seq` | 解析した各項目の `AsyncIterable` |
| `text/event-stream` | `AsyncIterable<{ data, event?, id?, retry? }>` |
| `*/*`、`application/*`、範囲としての `text/*` | 型は `unknown`（範囲にはテキストのほか、イベントストリームや JSON も含まれるため）。実際の `Content-Type` に基づいてデコード |
| その他 | `Blob` |

`HEAD` リクエストと、ステータスコードが `204`、`205`、`304` のレスポンスでは、`data` は `undefined` です。また、`Content-Type` ヘッダーのないレスポンスはテキストとして読み込まれ、本体が空のときは `undefined` になります。スキーマに `type` がない `text/*` のレスポンス（OpenAPI 3.0 では `type: string, format: binary`）は、ダウンロードする CSV ファイルなどのバイナリーデータとして扱われ、`data` は `Blob` になります。これは、そのように宣言したレスポンスにだけ当てはまります。同じメディアタイプでも、`type` のあるスキーマを持つほかのレスポンスは、テキストとして読み込まれます。スキーマのない `text/*` のレスポンスは、テキストとして読み込まれます。逐次的なメディアタイプ（sequential media types）の各項目は、ドキュメントに `itemSchema` があるときは、その `itemSchema` に基づいて型付けされます。

## エラー {#errors}

ステータスコードが 2xx 以外のレスポンスは、例外として投げられずにそのまま返されます。呼び出しが例外を投げるのは、次のときだけです。

### ネットワークエラー {#network-errors}

接続の失敗やリクエストの中止など、`fetch` が投げるエラーは、呼び出しからそのまま投げられます。

### `DecodeError` {#decodeerror}

HTML のエラーページが `application/json` として返されたときなど、レスポンス本体がその `Content-Type` と一致しないときは、`DecodeError` が投げられます。`DecodeError` の `response` プロパティにはレスポンスオブジェクトが格納されており、その本体はすでに消費されています。また、`text` プロパティには、デコードに失敗したテキストが格納されています。なお、逐次的なメディアタイプでは、反復処理の途中で `DecodeError` が投げられ、`text` にはデコードに失敗した項目が入ります。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
import { DecodeError } from "./api.ts";

try {
  await api.GET("/pets");
} catch (error) {
  if (error instanceof DecodeError) {
    console.error(error.response.status, error.text);
  } else {
    throw error;
  }
}
```

### `TypeError` {#typeerror}

パスパラメーターによってパスのセグメントが空、`.`、`..` のいずれかになるときは、リクエストを送信する前に `TypeError` が投げられます。

## クライアントオブジェクト {#the-client-object}

クライアントは、ドキュメントにある HTTP メソッドごとのプロパティを、HTTP と同じ大文字・小文字の表記で持ち（`api.get` ではなく `api.GET`）、そのほかに `with` メソッドも持ちます（[ミドルウェア](./middleware#per-client-middleware-with-with)を参照）。小文字のメソッド名を含め、それ以外のプロパティは通常のオブジェクトと同じように振る舞うため、クライアントの内容を調べたりシリアライズしたりしても、リクエストが送信されることはありません。
