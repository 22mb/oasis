# Strict types

The types that oasis-gen generates follow the text and intent of the OpenAPI
specification. When the specification allows a status or a value, the
generated types include it, so the compiler makes you handle it. This page
shows the rules that follow from this principle.

The examples use a client generated from a petstore document. Hover over a
name in a code block to see its type.

## Responses are typed by status

The specification does not expect a document to cover every status that an
operation can return. oasis-gen therefore types `status` as every status that
a response can have, and each key under `responses` covers some of them:

| Key under `responses` | Statuses covered | `data` |
| --- | --- | --- |
| A status code, such as `200` | That code | The type of that response |
| A range, such as `4XX` | The codes in the range that are not declared on their own | The type of that response |
| `default` | Every status that no other key covers, including 2xx | The type of the `default` response |
| No key | The statuses that no key covers, when there is no `default` | `unknown` |

Checking `status` narrows `data` to the matching response:

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

Here the operation declares `200` and `404` but no `default`, so any other
status has `data: unknown`.

When an operation declares `default`, the `default` response also covers the
other successful statuses. `GET /pets` declares `200` and `default`, so a `201` response has the type
of the `default` response:

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

## `ok` is `response.ok`

`ok` is the `ok` property of the `Response`: `true` for any 2xx status. Because
a 2xx status can also be covered by `default` or by no key, checking `ok` does
not narrow `data` to the declared success type:

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

Check `status` instead, or `switch` on it, as in the examples above.

## Error responses are returned, not thrown {#error-responses-are-values}

Non-2xx responses are returned, not thrown. A call throws only for network
errors, for bodies that do not match their `Content-Type`, and for
invalid path parameters. See [Errors](./client#errors). To throw on an unexpected status,
see the [recipe](./recipes#throwing-on-unexpected-statuses).

## Requests and responses have separate types

The specification says that `readOnly` properties should not be sent in
requests, and that `writeOnly` properties should not be sent in responses. A
schema with such properties therefore gets two types: one for responses, and a
request variant, named with a `Request` suffix:

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

- In the request variant, `readOnly` properties have the type `never`, so you
  cannot set them. In the response type, `writeOnly` properties have the type
  `never`. In the other type, these properties stay required or optional as
  the schema declares.
- A property is also `readOnly` or `writeOnly` when its `$ref` target is, or
  when another `allOf` member marks a property of the same name.
- Request bodies use the request variant automatically. A schema that refers
  to a schema with a request variant also gets a request variant.

## Formats are not converted

A `format` does not change the type. A `date-time` stays a `string`, and an
`int64` is a `number`, which is what `JSON.parse` produces. Integers above
2<sup>53</sup> therefore lose precision. Convert values yourself where you need
another type. The exceptions are `format: binary` and `byte` in OpenAPI 3.0,
which describe binary data there; see [Versions](./openapi-support#versions).
