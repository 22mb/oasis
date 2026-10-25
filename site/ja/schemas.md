# スキーマ

このページでは、OpenAPI ドキュメントのスキーマに対して、生成されるファイルがエクスポートする TypeScript の型を説明します。

## エクスポートされる名前 {#exported-names}

コンポーネントのスキーマは、それぞれの名前でエクスポートされます。`enum` のスキーマについては、その値もドキュメントでの順序のまま、同じ名前の値としてエクスポートされます。

```ts twoslash
import { Kind, type Pet } from "./api.ts";

const kinds = Kind;
//    ^?
type PetKind = Pet["kind"];
//   ^?
```

- 名前は、スキーマのキーに含まれる英字・数字・`_` 以外の文字の連なりを、それぞれ 1 つの `_` に置き換えたものです。たとえば `Pet.Owner` は `Pet_Owner` になります。また、数字で始まるキーには、先頭に `_` が付きます。
- クライアント自体のエクスポート（`Client`、`Middleware` など）や先に現れたスキーマですでに使われている名前には、番号が付きます。たとえば `Client` という名前のスキーマは `Client_2` になります。
- [リクエスト用の型](./strict-types#requests-and-responses-have-separate-types)を持つスキーマについては、そのリクエスト用の型も、`AccountRequest` のように `Request` 接尾辞を付けた名前でエクスポートされます。

## スキーマと TypeScript の型の対応 {#how-schemas-map-to-typescript}

| スキーマ | TypeScript |
| --- | --- |
| `type: string`（`format` は問わない。`date-time`、`uuid`、`binary` など） | `string` |
| `type: integer` または `number`（`format` は問わない。`int64` など） | `number` |
| `type: boolean` | `boolean` |
| `type: "null"`、または OpenAPI 3.0 の `nullable: true` | `null` を追加 |
| `enum`、`const` | リテラル型のユニオン型、または 1 つのリテラル型 |
| `type: array` | `T[]`。`prefixItems` があるときはタプル型 |
| `properties` を持つ `type: object` | オブジェクト型。`required` にないプロパティは省略可能 |
| `additionalProperties` | インデックスシグネチャ（オブジェクトが `properties` も持つときは `unknown`） |
| `patternProperties` | すべてのキーを対象とするインデックスシグネチャ（TypeScript ではキーをパターンで絞れないため）。値の型は、各パターンと `additionalProperties` の型のユニオン型（オブジェクトが `properties` も持つとき、または `additionalProperties` がないときは `unknown`） |
| `oneOf`、`anyOf` | ユニオン型。`discriminator` があるときは、各メンバーにその値のタグを付与 |
| `allOf` | インターセクション型 |
| コンポーネントへの `$ref` | そのスキーマの型 |
| `required` にだけ含まれるプロパティ | `{} \| null`（`undefined` 以外の任意の値） |
| スキーマなし、または `{}` | `unknown` |

`title`、`description`、`default`、`deprecated` は JSDoc コメントになるため、エディターでホバーしたときに表示されます。なお、`format` に応じた変換は行われません。詳しくは[厳密な型](./strict-types#formats-are-not-converted)をご覧ください。

## サブタイプ {#subtypes}

スキーマによっては、`oneOf` にサブタイプを列挙し、各サブタイプが `allOf` で親を拡張しています。springdoc は Jackson のサブタイプに対してこのパターンを出力します。このような親スキーマは、サブタイプのユニオン型になります。TypeScript はサブタイプから親への参照を解決できないため、各サブタイプには、親への参照の代わりに親自身のプロパティが含まれます。

```yaml
Animal:
  type: object
  required: [kind]
  properties:
    kind: { type: string }
  discriminator:
    propertyName: kind
  oneOf:
    - $ref: "#/components/schemas/Dog"
    - $ref: "#/components/schemas/Cat"
Dog:
  allOf:
    - $ref: "#/components/schemas/Animal"
    - type: object
      properties:
        bark: { type: boolean }
# Cat は Dog と同様に定義し、`lives: { type: integer }` を持つ。
```

```ts twoslash
import type { Animal } from "./features.ts";
declare const animal: Animal;
// ---cut---
if (animal.kind === "Dog") {
  animal.bark;
  //     ^?
}
```

`kind` を確認すると、`Animal` は `Dog` か `Cat` に絞り込まれます。

oasis-gen は、`oneOf` を持たず、サブタイプから `allOf` で参照されるだけの親に置かれた `discriminator` には対応していません。このような親は、通常のオブジェクト型のままです。

## 型エイリアスと `components` {#type-aliases-and-components}

各スキーマは、`$Pet` のように `$` 接頭辞を付けた型エイリアスとして定義されます。リクエスト用の型は `$Pet$Request` です。ホバーしたときの表示やエラーメッセージには、これらの名前が使われます。エクスポートされる名前、`schemas`、`components` は、いずれもこれらの型エイリアスを参照します。

| 型 | 内容 |
| --- | --- |
| `schemas["Pet"]` または `components["schemas"]["Pet"]` | `Pet` と同じ型 |
| `components["requestSchemas"]` | コンポーネントのスキーマのリクエスト用の型 |
