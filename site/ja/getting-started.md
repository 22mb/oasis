# クイックスタート

このページでは、OpenAPI ドキュメントから型付きの API を呼び出すまでの手順を説明します。

Node.js 22 以降と TypeScript プロジェクトが必要です。生成されるファイルに必要な TypeScript の設定については、[動作要件](./reference#requirements)をご覧ください。

## oasis-gen のインストール {#install-oasis-gen}

```sh
npm install --save-dev oasis-gen
```

## ドキュメントの作成 {#write-a-document}

次の OpenAPI ドキュメントを `openapi.yaml` として保存します。このドキュメントには、ペットを返すか、ペットが存在しないときにエラーを返す操作が 1 つ記述されています。

```yaml
openapi: 3.2.0
info: { title: Petstore, version: 1.0.0 }
servers:
  - url: https://api.example.com
paths:
  /pets/{petId}:
    get:
      operationId: getPet
      parameters:
        - { name: petId, in: path, required: true, schema: { type: integer } }
      responses:
        "200":
          description: The pet.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Pet" }
        "404":
          description: The pet does not exist.
          content:
            application/problem+json:
              schema: { $ref: "#/components/schemas/Error" }
components:
  schemas:
    Pet:
      type: object
      required: [id, name]
      properties:
        id: { type: integer }
        name: { type: string }
    Error:
      type: object
      required: [title]
      properties:
        title: { type: string }
```

## クライアントの生成 {#generate-the-client}

ドキュメントを変更するたびにクライアントを再生成できるよう、`package.json` にスクリプトを追加します。

```json
{
  "scripts": {
    "generate": "oasis-gen openapi.yaml -o src/api.ts"
  }
}
```

次に、このスクリプトを実行します。

```sh
npm run generate
```

これで、`src/api.ts` にすべての操作の型と `fetch` ベースの小さなランタイムが出力されます。このファイルには依存関係がないため、ほかにパッケージをインストールする必要はありません。

## 操作の呼び出し {#call-the-operation}

```ts twoslash
import { createClient } from "./api.ts";

const api = createClient();

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
switch (res.status) {
  case 200:
    console.log(res.data.name);
    //              ^?
    break;
  case 404:
    console.log(res.data.title);
    //              ^?
    break;
  default:
    console.error(res.status, res.data);
    //                            ^?
}
```

操作は、HTTP メソッドとパステンプレートをドキュメントの記述どおりに指定して選択します。`status` を判定すると、`data` の型はドキュメントでそのステータスに対して宣言されているレスポンスに絞り込まれます。一方、`default` の分岐ではドキュメントで宣言されていないステータスを扱うため、`data` の型は `unknown` になります。その理由は[厳密な型](./strict-types)で説明しています。

## 次のステップ {#next-steps}

- [厳密な型](./strict-types)：レスポンスとリクエストの型付けの仕組み
- [操作の呼び出し](./client)：パラメーター、リクエスト本体、レスポンス、エラーの扱い方
- [認証](./authentication)：ドキュメントの `security` で要求される資格情報の送信方法
- [レシピ](./recipes)：エラー時に例外を投げる方法、TanStack Query、モックなど
