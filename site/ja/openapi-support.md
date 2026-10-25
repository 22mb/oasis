# OpenAPI の対応状況

このページでは、oasis-gen が OpenAPI ドキュメントから何を受け付け、それぞれの機能をどう扱うかを一覧にしています。対応状況を表す語の意味は次のとおりです。

- **対応**：生成されるクライアントに影響する機能
- **無視**：受け付けるが、生成されるクライアントには影響しない機能
- **拒否**：エラーとなり、生成が中止される機能
- **未対応**：「備考」欄の記載どおりに解釈されるか、まったく読み込まれない機能

## バージョン {#versions}

| バージョン | 対応状況 |
| --- | --- |
| OpenAPI 3.2 | 対応 |
| OpenAPI 3.1 | 対応（3.2 とまったく同じように解釈） |
| OpenAPI 3.0 | 対応（下記の違いあり） |
| Swagger 2.0（`swagger: "2.0"`） | 未対応 |

OpenAPI 3.0 のドキュメントでは、これらのキーワードを次のように扱います。

| キーワード | 扱い |
| --- | --- |
| `nullable: true` | 型に `null` を追加。`enum` に付いているときは、列挙に `null` がなくても `null` を追加。ほかの生成ツールと同じ解釈だが、3.0.3 自体の規定ではこのような `enum` は `null` を含まない |
| `$ref` と並んだ `nullable: true`、または `$ref` を 1 つだけ含む `allOf` に付いた `nullable: true` | 参照先の型に `null` を追加 |
| `type: string, format: binary` | 生のバイナリーデータ。`type` のないスキーマとして解釈し、`unknown`（マルチパートの本体では `Blob`）になる |
| `type: string, format: byte` | Base64 のテキスト。`contentEncoding: base64` として解釈し、`string` になる。マルチパートのパートは `application/octet-stream` として送信 |
| `example`、真偽値の `exclusiveMinimum` | 無視 |

3.1 または 3.2 のドキュメントでは、仕様書に従うと `nullable` が警告なく無視されることになるため、`nullable` は拒否されます。代わりに `type: [..., "null"]` を使用してください。

## ドキュメント {#documents}

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| JSON と YAML | 対応 | ファイルから、または `-` の指定で標準入力から読み込み |
| ほかのローカルファイルへの `$ref` | 対応 | ドキュメント全体またはその一部。参照元のドキュメントからの相対パスで解決 |
| `$self`、スキーマ内の `$id` と `$anchor` | 対応 | `https://api.example.com/openapi/root.yaml` のような `$self` の URI をそのファイル自身のディレクトリーに対応付けるため、隣にある `schemas/pet.yaml` はローカルのファイルとして検出 |
| `components.mediaTypes` とそれへの `$ref` | 対応 | |
| リモートの `$ref` と、入力としての URL | 未対応 | ローカルのファイルに対応付けられない `https://...` の URI |

ほかのファイルのスキーマも型エイリアスとして定義されますが、エクスポートされて `components` に含まれるのは、コンポーネントのスキーマだけです（[スキーマ](./schemas#type-aliases-and-components)を参照）。

## スキーマ {#schemas}

各キーワードがどの TypeScript の型になるかは、[スキーマ](./schemas)をご覧ください。

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| JSON Schema 2020-12 | 対応 | `type` の配列、`const`、`enum`、`allOf`、`oneOf`、`anyOf`、`prefixItems`、`additionalProperties`、`patternProperties`、ほかのキーワードと並んだ `$ref`、`readOnly` と `writeOnly`（[厳密な型](./strict-types#requests-and-responses-have-separate-types)を参照） |
| 再帰的なスキーマ | 対応 | プロパティ、配列、`additionalProperties` を経由するもの。`$defs` をはじめとする `components.schemas` 以外の場所への `$ref` も含む |
| `oneOf` または `anyOf` の `discriminator` | 対応 | 明示的なマッピングと暗黙のマッピング、`defaultMapping` |
| 親の `oneOf` に列挙され、それぞれが `allOf` で親を拡張するサブタイプ | 対応 | [サブタイプ](./schemas#subtypes)を参照 |
| `$ref`、`allOf`、`oneOf`、`anyOf` だけを経由するその他の循環 | 拒否 | 間にオブジェクトも配列も挟まない循環 |
| `allOf` の親に付いた `discriminator` | 未対応 | 親は子のユニオン型に変換されない |
| `$dynamicRef` | 未対応 | `unknown` として解釈 |

## 操作 {#operations}

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| `get`、`put`、`post`、`delete`、`options`、`head`、`patch`、`trace` | 対応 | |
| `query`（HTTP の `QUERY` メソッド） | 対応 | |
| `additionalOperations` | 対応 | `LINK` などの独自のメソッド |
| 一部の `additionalOperations` の名前 | 拒否 | 大文字・小文字を問わない `delete`、`get`、`head`、`options`、`post`、`put` と、大文字の `PATCH`、`TRACE`、`QUERY`（fetch がこれらを固定フィールドのメソッドとして送信することになるため）。また、クライアントが予約している `with` と `then` |
| コールバックと Webhook | 未対応 | |

## パラメーター {#parameters}

| 場所 | スタイル | 備考 |
| --- | --- | --- |
| `query` | `form`、`deepObject` | `allowReserved` に対応。`spaceDelimited` と `pipeDelimited` は拒否 |
| `path` | `simple` | `label` と `matrix` は拒否 |
| `header` | `simple` | |
| `cookie` | `form`（パーセントエンコード）、`cookie`（そのまま送信） | 仕様書ではクッキーに `&` を使えないため、`explode` で展開された値はどちらのスタイルでも `; ` で連結 |
| `querystring` | 対応 | メディアタイプに基づいて型付けとエンコードを実施。[`querystring` パラメーター](./client#querystring-parameters)を参照 |

どのスタイルも `explode` に対応しています。また、スキーマの代わりに `content` を持つパラメーターにも対応しています。メディアタイプが JSON のクエリーパラメーターは、スキーマから型付けされて JSON としてシリアライズされます。一方、`content` を持つその他のパラメーターには、シリアライズ済みの文字列を渡します。

クッキーパラメーターとクッキーで送信する API キーは `Cookie` ヘッダーとして送信されますが、ブラウザーは `fetch` で設定されたこのヘッダーを無視します。一方、Node.js と Bun では機能します。

## リクエスト本体とレスポンス本体 {#request-and-response-bodies}

各メディアタイプの型付けとデコードの方法は、[リクエスト本体](./client#request-bodies)と[レスポンス本体](./client#response-bodies)をご覧ください。

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| 逐次的なメディアタイプ（sequential media types）と `itemSchema` | 対応 | JSON Lines、NDJSON、JSON テキストシーケンス、サーバー送信イベント。非同期反復可能オブジェクトとしてストリーミング |
| `multipart/form-data` のリクエスト本体 | 対応 | スキーマから型付けし、`FormData` として送信。[フォームの本体](./client#form-bodies)を参照 |
| その他の `multipart` のリクエスト本体（`multipart/mixed` など） | 未対応 | `FormData` として渡す。`FormData` は常に `multipart/form-data` として送信される |
| Encoding Object（`encoding`） | 対応 | `contentType`、`style`、`explode`、`allowReserved` に対応。`FormData` ではパートのヘッダーや入れ子のパートを送信できないため、省略可能な `headers` は送信せず、必須のヘッダーと `multipart` のパートは拒否 |
| `prefixEncoding`、`itemEncoding` | 未対応 | その他の `multipart` のメディアタイプにだけ適用される |
| 型付きのレスポンスヘッダー | 未対応 | |

## サーバーとセキュリティ {#servers-and-security}

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| 最上位、パスアイテム、操作の `servers` | 対応 | 変数には既定値を設定。[サーバーと `baseUrl`](./client#servers-and-baseurl)を参照 |
| セキュリティスキーム | 対応 | `apiKey`、`http`、`oauth2`、`openIdConnect`、`mutualTLS`。[`auth`](./authentication) を通じて利用 |
| OAuth2 のフロー | 未対応 | トークンは利用者自身が取得 |
