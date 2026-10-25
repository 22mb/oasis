# 認証

`auth` は、各操作の `security` の指定どおりに認証を行うミドルウェアです。利用者が用意するのは、それぞれの資格情報を取得する方法だけです。

```ts twoslash
// @filename: api.ts
export * from "./security.ts";
// @filename: index.ts
declare function getToken(scopes: string[]): Promise<string | undefined>;
declare const process: { env: Record<string, string | undefined> };
// ---cut---
import { auth, createClient } from "./api.ts";

const api = createClient({
  middleware: [
    auth({
      // キーには components.securitySchemes のスキーム名を使う。
      bearer: (scopes) => getToken(scopes),
      apiKey: () => process.env.API_KEY,
    }),
  ],
});
```

## 資格情報の選択 {#choosing-credentials}

キーには `components.securitySchemes` の名前（型は `SecuritySchemeName`）を指定します。値はそれぞれ `Credential` 型で、文字列か、文字列を返す関数（非同期でも可）です。関数はリクエストごとに、セキュリティ要件に列挙されたスコープと[ミドルウェアのコンテキスト](./middleware#the-operation)を引数として呼び出されます。資格情報を利用できないときは、関数から `undefined` か `""` を返します。

`auth` はリクエストごとに、操作の `security` に並ぶ選択肢のうち、資格情報がすべて利用可能な最初の選択肢を選びます。リクエストに何も追加しない選択肢（匿名の `{}` か、`mutualTLS` だけの選択肢）は、ほかに利用可能な選択肢がないときに限って使われます。どの選択肢も利用できないときは、リクエストは資格情報なしで送信され、可否はサーバーが判断します。

## 資格情報の送信先 {#where-credentials-go}

| セキュリティスキーム | 送信形式 |
| --- | --- |
| `http`（`bearer`）、`oauth2`、`openIdConnect` | `Authorization: Bearer <credential>` |
| `http`（`basic`） | `Authorization: Basic <credential, base64-encoded>`（資格情報は `user:password` 形式） |
| その他の `http` スキーム | `Authorization: <scheme> <credential>` |
| `apiKey`（ヘッダー） | 指定された名前のヘッダー |
| `apiKey`（クエリーまたはクッキー） | 指定された名前のクエリーパラメーターまたはクッキー（パーセントエンコード済み） |
| `mutualTLS` | なし。`fetch` の実装側で TLS を設定 |

ブラウザーは `fetch` で設定した `Cookie` ヘッダーを無視するため、クッキーで送信する API キーは Node.js、Deno、Bun でのみ機能します。

## 資格情報が送信されないとき {#when-nothing-is-sent}

`auth` は OpenAPI ドキュメントの `security` だけに従います。`security: []` の操作には資格情報を何も送信しません。また、ドキュメントで `securitySchemes` を宣言していても `security` 要件を宣言していないときは、どの操作にも資格情報を何も送信しません。ドキュメントの記述にかかわらずすべてのリクエストにヘッダーを付けて送信するには、代わりにクライアントの `headers` オプションを使用してください。
