# Getting started

This page takes you from an OpenAPI document to a typed API call.

You need Node.js 24 or later and a TypeScript project. See
[Requirements](./reference#requirements) for the TypeScript settings that the
generated file needs.

## Install oasis-gen

```sh
npm install --save-dev oasis-gen
```

## Write a document

Save this OpenAPI document as `openapi.yaml`. It describes one operation, which
returns a pet, or an error when the pet does not exist.

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

## Generate the client

Add a script to `package.json` so that you can regenerate the client whenever
the document changes:

```json
{
  "scripts": {
    "generate": "oasis-gen openapi.yaml -o src/api.ts"
  }
}
```

Then run it:

```sh
npm run generate
```

`src/api.ts` now contains a type for every operation and a small `fetch`-based
runtime. The file has no dependencies, so there is nothing else to install.

## Call the operation

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

You select an operation by its HTTP method and path template, exactly as the
document writes them. Comparing `status` narrows `data` to the response that
the document declares for that status. The `default` branch handles the
statuses that the document does not declare, so `data` is `unknown` there.
[Strict types](./strict-types) explains why.

## Next steps

- [Strict types](./strict-types): how responses and requests are typed
- [Calling operations](./client): parameters, request bodies, responses, and
  errors
- [Authentication](./authentication): sending the credentials that the
  document's `security` requires
- [Recipes](./recipes): throwing on errors, TanStack Query, mocks, and more
