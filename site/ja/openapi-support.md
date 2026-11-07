# OpenAPI の対応状況

このページでは、oasis-gen が OpenAPI ドキュメントから何を受け付け、それぞれの機能をどのように扱うかを一覧にしています。対応状況を表す語の意味は次のとおりです。

- **対応**：生成されるクライアントに影響する機能
- **無視**：受け付けるが、生成されるクライアントには影響しない機能
- **拒否**：エラーとなり、クライアントの生成が中止される機能
- **未対応**：「備考」欄の記載どおりに解釈されるか、まったく処理されない機能

## バージョン {#versions}

| バージョン | 対応状況 |
| --- | --- |
| OpenAPI 3.2 | 対応 |
| OpenAPI 3.1 | 対応（3.2 とまったく同じように解釈） |
| OpenAPI 3.0 | 対応（下記の違いあり） |
| Swagger 2.0（`swagger: "2.0"`） | 未対応 |

OpenAPI 3.0 のドキュメントでは、以下のキーワードを次のように扱います。

| キーワード | 扱い |
| --- | --- |
| `nullable: true` | 型に `null` を追加。`enum` に指定されている場合は、列挙値に `null` がなくても `null` を追加（ほかの生成ツールも同様に解釈するが、3.0.3 自体の規定ではこのような `enum` に `null` は認められない） |
| `$ref` と並んだ `nullable: true`、または `$ref` を 1 つだけ含む `allOf` に指定された `nullable: true` | 参照先の型に `null` を追加 |
| `type: string, format: binary` | 生のバイナリーデータ。`type` のないスキーマとして解釈され、`unknown` 型（マルチパートの本体と `text/*` のレスポンスでは `Blob` 型）に変換 |
| `type: string, format: byte` | Base64 のテキスト。`contentEncoding: base64` として解釈され、`string` 型に変換。マルチパートのパートとしては `application/octet-stream` で送信 |
| `example`、論理値の `exclusiveMinimum` | 無視 |

3.1 または 3.2 のドキュメントでは、仕様上 `nullable` が警告なく無視されるため、`nullable` の使用は拒否されます。代わりに `type: [..., "null"]` を使用してください。

## ドキュメント {#documents}

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| JSON と YAML | 対応 | ファイルから、または `-` の指定により標準入力から読み込み |
| ほかのローカルファイルへの `$ref` | 対応 | ドキュメント全体またはフラグメント。参照元のドキュメントからの相対パスで解決 |
| `$self`、スキーマ内の `$id` と `$anchor` | 対応 | `https://api.example.com/openapi/root.yaml` のような `$self` の URI をそのファイル自身のディレクトリーに対応付けるため、同じ階層にある `schemas/pet.yaml` をローカルファイルとして検出 |
| `components.mediaTypes` とそれへの `$ref` | 対応 | |
| ASCII の英数字、`.`、`-`、`_` 以外の文字を含むコンポーネント名 | 拒否 | 仕様で `^[a-zA-Z0-9\.\-_]+$` に限定されている |
| リモートの `$ref`、入力としての URL | 未対応 | ローカルファイルに対応付けられない `https://...` の URI |

ほかのファイルのスキーマに対しても個別の型エイリアスが生成されますが、エクスポートされて `components` に含まれるのはコンポーネントのスキーマだけです（[スキーマ](./schemas#type-aliases-and-components)を参照）。

## スキーマ {#schemas}

各キーワードがどの TypeScript の型に変換されるかについては、[スキーマ](./schemas)をご覧ください。

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| JSON Schema 2020-12 | 対応 | `type` の配列、`const`、`enum`、`allOf`、`oneOf`、`anyOf`、`prefixItems`、`minItems`、`additionalProperties`、`patternProperties`、`propertyNames`（名前の列挙として）、ほかのキーワードと並んだ `$ref`、`readOnly` と `writeOnly`（[厳密な型](./strict-types#requests-and-responses-have-separate-types)を参照） |
| 再帰的なスキーマ | 対応 | プロパティ、配列、`additionalProperties` を経由する再帰。`$defs` など `components.schemas` 以外の場所への `$ref` も含む |
| `oneOf` または `anyOf` の `discriminator` | 対応 | 明示的なマッピングと暗黙的なマッピング、`defaultMapping` |
| 親スキーマの `oneOf` に列挙され、それぞれが `allOf` で親を拡張するサブタイプ | 対応 | [サブタイプ](./schemas#subtypes)を参照 |
| `$ref`、`allOf`、`oneOf`、`anyOf` のみを経由するその他の循環参照 | 拒否 | 間にオブジェクトも配列も挟まない循環参照 |
| `allOf` の親スキーマに指定された `discriminator` | 未対応 | 親スキーマは子スキーマのユニオン型には変換されない |
| `$dynamicRef` | 未対応 | `unknown` として型付け |

## 操作 {#operations}

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| `get`、`put`、`post`、`delete`、`options`、`head`、`patch`、`trace` | 対応 | |
| `query`（HTTP の `QUERY` メソッド） | 対応 | |
| `additionalOperations` | 対応 | `LINK` などのカスタムメソッド |
| 一部の `additionalOperations` の名前 | 拒否 | 大文字・小文字を問わない `delete`、`get`、`head`、`options`、`post`、`put` と、大文字の `PATCH`、`TRACE`、`QUERY`（`fetch` がこれらを固定フィールドと同じメソッドとして送信するため）。また、クライアント側で予約されている `with` と `then` |
| コールバックと Webhook | 未対応 | |

## パラメーター {#parameters}

| 場所 | スタイル | 備考 |
| --- | --- | --- |
| `query` | `form`、`deepObject` | `allowReserved` に対応。`spaceDelimited` と `pipeDelimited` は拒否 |
| `path` | `simple` | `label` と `matrix` は拒否 |
| `header` | `simple` | |
| `cookie` | `form`（パーセントエンコード）、`cookie`（そのまま送信） | 仕様上クッキーには `&` を使用できないため、`explode` で展開された値はどちらのスタイルでも `; ` で連結 |
| `querystring` | 対応 | メディアタイプに基づいて型付けとエンコードを実施。[`querystring` パラメーター](./client#querystring-parameters)を参照 |

すべてのスタイルが `explode` に対応しています。また、スキーマの代わりに `content` を持つパラメーターにも対応しています。メディアタイプが JSON のクエリーパラメーターは、スキーマに基づいて型付けされ、JSON としてシリアライズされます。一方、`content` を持つその他のパラメーターには、シリアライズ済みの文字列を渡します。

クッキーパラメーターとクッキーの API キーは `Cookie` ヘッダーとして送信されますが、ブラウザーの `fetch` ではこのヘッダーが無視されます。Node.js と Bun では動作します。

## リクエスト本体とレスポンス本体 {#request-and-response-bodies}

各メディアタイプの型付けとデコードの方法については、[リクエスト本体](./client#request-bodies)と[レスポンス本体](./client#response-bodies)をご覧ください。

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| 逐次的なメディアタイプ（sequential media types）と `itemSchema` | 対応 | JSON Lines、NDJSON、JSON テキストシーケンス、サーバー送信イベント。非同期反復可能オブジェクトとしてストリーミング |
| `multipart/form-data` のリクエスト本体 | 対応 | スキーマに基づいて型付けし、`FormData` として送信。[フォームの本体](./client#form-bodies)を参照 |
| その他の `multipart` のリクエスト本体（`multipart/mixed` など） | 未対応 | 利用者が組み立てた `Blob` として渡す（`type` にメディアタイプと `boundary` パラメーターを指定） |
| Encoding Object（`encoding`） | 対応 | `contentType`、`style`、`explode`、`allowReserved` に対応。`FormData` ではパートのヘッダーや入れ子のパートを送信できないため、省略可能な `headers` は除外され、必須のヘッダーや `multipart` のパートは拒否 |
| `prefixEncoding`、`itemEncoding` | 未対応 | その他の `multipart` メディアタイプにのみ適用される機能 |
| 型付きのレスポンスヘッダー | 未対応 | |

## サーバーとセキュリティ {#servers-and-security}

| 機能 | 対応状況 | 備考 |
| --- | --- | --- |
| 最上位、パスアイテム、操作の `servers` | 対応 | 変数には既定値を設定。[サーバーと `baseUrl`](./client#servers-and-baseurl)を参照 |
| セキュリティスキーム | 対応 | `apiKey`、`http`、`oauth2`、`openIdConnect`、`mutualTLS`。[`auth`](./authentication) を通じて利用 |
| OAuth2 のフロー | 未対応 | トークンは利用者自身で取得 |
