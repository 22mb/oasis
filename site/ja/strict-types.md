# 厳密な型

oasis-gen が生成する型は、OpenAPI 仕様書の記述と意図に従います。仕様書で許容されているステータスや値は生成される型にも含まれるため、それらを処理するようコンパイラーから求められます。このページでは、この原則から導かれる規則を説明します。

このページの例では、ペットストアのドキュメントから生成したクライアントを使います。コードブロック内の名前にホバーすると、その型が表示されます。

## ステータスに基づくレスポンスの型付け {#responses-are-typed-by-status}

仕様書では、操作が返し得るすべてのステータスをドキュメントが網羅することは想定されていません。そのため、oasis-gen はレスポンスが取り得るすべてのステータスを `status` の型とし、`responses` 配下の各キーにそのうちの一部のステータスが該当します。

| `responses` 配下のキー | 該当するステータス | `data` |
| --- | --- | --- |
| `200` などのステータスコード | そのステータスコード | そのレスポンスの型 |
| `4XX` などの範囲 | 範囲内のステータスコードのうち、個別に宣言されていないもの | そのレスポンスの型 |
| `default` | ほかのどのキーにも該当しないすべてのステータス（2xx を含み、`304` を除く） | `default` レスポンスの型 |
| キーなし | `default` がないときに、どのキーにも該当しないステータス（`304` を除く） | `unknown` |

`304` のレスポンスが型に含まれるのは、ドキュメントで `304` か `3XX` を宣言したときだけです。サーバーが `304` を返すのは条件付きリクエストへの応答に限られ、`fetch` がそれをコードに返すのは、`If-None-Match` などのヘッダーを自分で付けてリクエストを送ったときだけです。ブラウザーの HTTP キャッシュがレスポンスを再検証したときは、`fetch` はキャッシュしてある `200` を代わりに返します。

`status` を判定すると、`data` は一致するレスポンスの型に絞り込まれます。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.status === 200) {
  res.data;
  //  ^?
} else if (res.status === 404) {
  res.data;
  //  ^?
} else {
  res.data;
  //  ^?
}
```

この例の操作は `200` と `404` を宣言していますが、`default` を宣言していないため、それ以外のステータスでは `data: unknown` になります。

一方、操作が `default` を宣言しているときは、ほかの成功ステータスも `default` レスポンスに該当します。たとえば、`GET /pets` は `200` と `default` を宣言しているため、`201` のレスポンスは `default` レスポンスの型になります。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
const res = await api.GET("/pets");
if (res.status === 201) {
  res.data;
  //  ^?
}
```

## `ok` は `response.ok` と同じ値 {#ok-is-response-ok}

`ok` は `Response` の `ok` プロパティであり、2xx のステータスであれば `true` になります。2xx のステータスは、`default` に該当することも、どのキーにも該当しないこともあるため、`ok` を判定しても `data` は宣言された成功時の型には絞り込まれません。

```ts twoslash
import { createClient } from "./api.ts";
const api = createClient();
// ---cut---
const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.ok) {
  res.data;
  //  ^?
}
```

代わりに、上の例のように `status` を比較するか、`switch` 文で判定してください。

## エラーレスポンスは例外にならずに返される {#error-responses-are-values}

2xx 以外のレスポンスは、例外として投げられずに値として返されます。呼び出しが例外を投げるのは、ネットワークエラーが発生したとき、レスポンス本体が `Content-Type` と一致しないとき、またはパスパラメーターが不正なときだけです。詳しくは[エラー](./client#errors)をご覧ください。また、想定外のステータスで例外を投げる方法については、[レシピ](./recipes#throwing-on-unexpected-statuses)をご覧ください。

## リクエストとレスポンスで分かれる型 {#requests-and-responses-have-separate-types}

仕様書では、`readOnly` のプロパティはリクエストで送信すべきではなく、`writeOnly` のプロパティはレスポンスで送信すべきではないと定められています。そのため、こうしたプロパティを持つスキーマからは、レスポンス用の型と、名前の末尾に `Request` が付くリクエスト用の型の 2 つが生成されます。

```yaml
Account:
  type: object
  required: [id, name, password]
  properties:
    id: { type: integer, readOnly: true }
    name: { type: string }
    password: { type: string, writeOnly: true }
```

```ts twoslash
import type { Account, AccountRequest } from "./features.ts";
// ---cut---
type InResponses = Account;
//   ^?

type InRequests = AccountRequest;
//   ^?
```

- リクエスト用の型では `readOnly` のプロパティの型が `never` になるため、値を設定できません。一方、レスポンス用の型では `writeOnly` のプロパティの型が `never` になります。もう一方の型（`readOnly` ならレスポンス用、`writeOnly` ならリクエスト用の型）では、これらのプロパティが必須か省略可能かはスキーマの宣言に従います。
- `$ref` の参照先が `readOnly` や `writeOnly` であるときや、`allOf` の別のメンバーが同じ名前のプロパティにこれらを指定しているときも、そのプロパティは `readOnly` や `writeOnly` として扱われます。
- リクエスト本体には、リクエスト用の型が自動的に使われます。また、リクエスト用の型を持つスキーマを参照するスキーマにも、リクエスト用の型が生成されます。

## `format` に応じた変換は行われない {#formats-are-not-converted}

`format` の指定によって型は変わりません。`date-time` は `string` のままで、`int64` は `JSON.parse` が返すとおり `number` になります。そのため、2<sup>53</sup> を超える整数では精度が失われます。別の型が必要なときは、値を自分で変換してください。ただし、OpenAPI 3.0 における `format: binary` と `byte` はバイナリーデータを表すため例外です。詳しくは[バージョン](./openapi-support#versions)をご覧ください。
