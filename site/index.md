---
layout: home

hero:
  name: oasis-gen
  text: A minimal API client generator for OpenAPI 3.2
  tagline: One dependency-free TypeScript file, with types for every operation and a small fetch-based runtime.
  image:
    src: /logo.svg
    alt: ""
  actions:
    - theme: brand
      text: Get started
      link: /getting-started
    - theme: alt
      text: GitHub
      link: https://github.com/22mb/oasis

features:
  - icon:
      src: /icons/spec.svg
    title: OpenAPI 3.2
    details: QUERY operations, sequential media types, multi-file documents, and JSON Schema 2020-12. oasis-gen also accepts OpenAPI 3.0 and 3.1 documents.
  - icon:
      src: /icons/strict.svg
    title: Strict types
    details: <code>status</code> narrows <code>data</code> to the declared response. Statuses that the document does not declare stay in the type. Request types follow <code>readOnly</code> and <code>writeOnly</code>.
  - icon:
      src: /icons/file.svg
    title: Zero runtime dependencies
    details: The output is a single TypeScript file that runs anywhere fetch is available. There is no runtime package to install and nothing to configure.
---

## Example

```sh
npx oasis-gen openapi.yaml -o src/api.ts
```

```ts twoslash
import { createClient } from "./api.ts";

const api = createClient({ baseUrl: "https://api.example.com" });

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.status === 200) {
  res.data.name; // typed from the 200 response
} else if (res.status === 404) {
  res.data.title; // typed from the 404 response
}
```
