---
layout: home

hero:
  name: oasis-gen
  text: OpenAPI 3.2 向けの最小限の API クライアント生成ツール
  tagline: 出力は依存関係のない TypeScript ファイル 1 つで、すべての操作の型と fetch ベースの小さなランタイムを含みます。
  image:
    src: /logo.svg
    alt: ""
  actions:
    - theme: brand
      text: はじめる
      link: /ja/getting-started
    - theme: alt
      text: GitHub
      link: https://github.com/22mb/oasis

features:
  - icon:
      src: /icons/spec.svg
    title: OpenAPI 3.2
    details: QUERY 操作、逐次的なメディアタイプ（sequential media types）、複数のファイルに分割した OpenAPI ドキュメント、JSON Schema 2020-12 に対応しています。また、oasis-gen は OpenAPI 3.0 と 3.1 のドキュメントも受け付けます。
  - icon:
      src: /icons/strict.svg
    title: 厳密な型
    details: <code>status</code> によって <code>data</code> は宣言されたレスポンスの型に絞り込まれます。ドキュメントで宣言されていないステータスも型に残ります。また、リクエストの型には <code>readOnly</code> と <code>writeOnly</code> が反映されます。
  - icon:
      src: /icons/file.svg
    title: 実行時の依存関係ゼロ
    details: 出力は 1 つの TypeScript ファイルで、fetch を使用できる環境ならどこでも動作します。インストールするランタイムパッケージも、設定する項目もありません。
---

## 例 {#example}

```sh
npx oasis-gen openapi.yaml -o src/api.ts
```

```ts twoslash
import { createClient } from "./api.ts";

const api = createClient({ baseUrl: "https://api.example.com" });

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.status === 200) {
  res.data.name; // 200 のレスポンスから型付けされる
} else if (res.status === 404) {
  res.data.title; // 404 のレスポンスから型付けされる
}
```
