<div align="center">
  <img src="https://raw.githubusercontent.com/22mb/oasis/main/site/public/logo-512.png" alt="" width="96" height="96">
  <h1>oasis-gen</h1>
  <p>A minimal API client generator for <b>OpenAPI 3.2</b>.</p>
  <p>
    <a href="https://22mb.github.io/oasis/"><b>Documentation</b></a> ·
    <a href="https://22mb.github.io/oasis/playground"><b>Playground</b></a> ·
    <a href="https://22mb.github.io/oasis/ja/">日本語</a>
  </p>
</div>

oasis-gen reads an OpenAPI document and writes one dependency-free TypeScript
file, with types for every operation and a small `fetch`-based runtime.

<img src="https://raw.githubusercontent.com/22mb/oasis/main/site/public/strict-types.png" alt="In the generated client, res.data has the type $Pet when res.status is 200, $Error when it is 404, and unknown for any other status">

- **OpenAPI 3.2**: `QUERY` operations, sequential media types (JSON Lines,
  server-sent events), multi-file documents, and JSON Schema 2020-12.
  OpenAPI 3.0 and 3.1 documents are accepted too.
- **Strict types**: `status` narrows `data` to the declared response.
  Statuses that the document does not declare stay in the type. Request types
  respect `readOnly` and `writeOnly`.
- **Zero runtime dependencies**: the output runs anywhere `fetch` is
  available. There is no runtime package to install and nothing to configure.

## Example

```sh
npx oasis-gen openapi.yaml -o src/api.ts
```

```ts
import { createClient } from "./api.ts";

const api = createClient({ baseUrl: "https://api.example.com" });

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.status === 200) {
  res.data.name; // typed from the 200 response
} else if (res.status === 404) {
  res.data.title; // typed from the 404 response
}
```

## Install

```sh
npm install --save-dev oasis-gen
```

The CLI and the generated client need Node.js 22 or later. The client also
runs in browsers and Bun, and type-checks with TypeScript 5.0 or later.

The package ships prebuilt binaries for macOS, Linux, and Windows, on arm64
and x64. npm installs only the one for your platform. Archives are also
attached to each [GitHub release](https://github.com/22mb/oasis/releases).
To build from source instead:

```sh
cargo install --git https://github.com/22mb/oasis
```

## Documentation

- [Getting started](https://22mb.github.io/oasis/getting-started): from an
  OpenAPI document to a typed API call
- [Strict types](https://22mb.github.io/oasis/strict-types): the rules that
  the generated types follow
- [Calling operations](https://22mb.github.io/oasis/client),
  [Middleware](https://22mb.github.io/oasis/middleware), and
  [Authentication](https://22mb.github.io/oasis/authentication)
- [Recipes](https://22mb.github.io/oasis/recipes): common tasks and
  troubleshooting
- [CLI and API](https://22mb.github.io/oasis/reference),
  [Schemas](https://22mb.github.io/oasis/schemas), and
  [OpenAPI support](https://22mb.github.io/oasis/openapi-support)

Changes in each release are listed in [CHANGELOG.md](CHANGELOG.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup and the
design of the generator.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
