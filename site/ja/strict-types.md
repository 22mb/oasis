# 厳密な型

oasis-gen が生成する型は、OpenAPI 仕様書の記述に従います。仕様書で許容されているステータスや値は生成される型にも含まれるため、それらを処理するようコンパイラーから求められます。このページでは、この原則から導かれる規則を説明します。

このページの例では、ペットストアのドキュメントから生成したクライアントを使います。コードブロック内の名前にホバーすると、その型が表示されます。

## ステータスに基づくレスポンスの型付け {#responses-are-typed-by-status}

仕様書では、操作が返しうるすべてのステータスをドキュメントが網羅することは想定されていません。そのため、oasis-gen はレスポンスが取りうるすべてのステータスを `status` の型とします。`responses` の下の各キーは、そのうちの一部を対象とします。

| `responses` の下のキー | 対象とするステータス | `data` |
| --- | --- | --- |
| `200` などのステータスコード | そのコード | そのレスポンスの型 |
| `4XX` などの範囲 | 範囲内のコードのうち、個別に宣言されていないもの | そのレスポンスの型 |
| `default` | ほかのどのキーも対象としないすべてのステータス（2xx を含む） | `default` レスポンスの型 |
| キーなし | `default` がないときに、どのキーも対象としないステータス | `unknown` |

`status` を比較すると、`data` は一致するレスポンスの型に絞り込まれます。

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

操作が `default` を宣言しているときは、`default` レスポンスがほかの成功ステータスも対象とします。`GET /pets` は `200` と `default` を宣言しているため、`201` のレスポンスは `default` レスポンスの型になります。

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

`ok` は `Response` の `ok` プロパティであり、2xx のステータスであれば `true` になります。2xx のステータスは、`default` が対象とすることも、どのキーも対象としないこともあります。そのため、`ok` を確認しても `data` は宣言された成功時の型には絞り込まれません。

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

代わりに、上の例のように `status` を比較するか、`status` を `switch` 文で判定してください。

## エラーレスポンスは値として返される {#error-responses-are-values}

呼び出しは、2xx 以外のレスポンスを例外として投げずに、値として返します。呼び出しが例外を投げるのは、ネットワークエラーのとき、本体が `Content-Type` と一致しないとき、パスパラメーターが不正なときだけです。詳しくは[エラー](./client#errors)をご覧ください。想定外のステータスで例外を投げる方法は、[レシピ](./recipes#throwing-on-unexpected-statuses)をご覧ください。

## リクエストとレスポンスで別々の型 {#requests-and-responses-have-separate-types}

仕様書では、`readOnly` のプロパティはリクエストで送信すべきではなく、`writeOnly` のプロパティはレスポンスで送信すべきではないとされています。そのため、こうしたプロパティを持つスキーマには 2 つの型が生成されます。1 つはレスポンス用の型で、もう 1 つは名前の末尾に `Request` が付くリクエスト用の型です。

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

- リクエスト用の型では `readOnly` のプロパティの型が `never` になるため、値を設定できません。レスポンス用の型では、`writeOnly` のプロパティの型が `never` になります。もう一方の型（`readOnly` ならレスポンス用、`writeOnly` ならリクエスト用の型）では、これらのプロパティが必須か省略可能かはスキーマの宣言どおりです。
- `$ref` の参照先が `readOnly` または `writeOnly` であるときや、`allOf` の別のメンバーが同じ名前のプロパティにこれらを指定しているときも、そのプロパティは `readOnly` または `writeOnly` になります。
- リクエスト本体には、リクエスト用の型が自動的に使われます。リクエスト用の型を持つスキーマを参照するスキーマにも、リクエスト用の型が生成されます。

## `format` に応じた変換は行われない {#formats-are-not-converted}

`format` によって型は変わりません。`date-time` は `string` のままで、`int64` は `JSON.parse` が生成するとおり `number` になります。そのため、2<sup>53</sup> を超える整数では精度が失われます。別の型が必要なときは、値を自分で変換してください。
