# Schemas

This page describes the TypeScript types that the generated file exports for
the document's schemas.

## Exported names

Each component schema is exported under its name. An enum schema also exports
its values, in document order, as a value with the same name:

```ts twoslash
import { Kind, type Pet } from "./api.ts";

const kinds = Kind;
//    ^?
type PetKind = Pet["kind"];
//   ^?
```

- A name is the schema's key with every run of characters other than letters,
  digits, and `_` replaced by one `_`: `Pet.Owner` becomes `Pet_Owner`. A key
  that starts with a digit gets a leading `_`.
- A name that is already taken by the client's own exports (`Client`,
  `Middleware`, ...) or by an earlier schema gets a numeric suffix: a schema
  named `Client` becomes `Client_2`.
- A schema with a [request variant](./strict-types#requests-and-responses-have-separate-types)
  also exports the request variant, named with a `Request` suffix, such as
  `AccountRequest`.

## How schemas map to TypeScript

| Schema | TypeScript |
| --- | --- |
| `type: string`, any `format` (`date-time`, `uuid`, ...) | `string` |
| `type: integer` or `number`, any `format` (`int64`, ...) | `number` |
| `type: boolean` | `boolean` |
| `type: "null"`, or `nullable: true` in OpenAPI 3.0 | Adds `null` |
| `enum`, `const` | A union of literals, or one literal |
| `type: array` | `T[]`; with `prefixItems`, a tuple |
| `type: object` with `properties` | An object type; properties not in `required` are optional |
| `additionalProperties` | An index signature (`unknown` when the object also has `properties`) |
| `patternProperties` | An index signature over every key, since TypeScript cannot restrict keys to a pattern: the patterns' types and that of `additionalProperties` (`unknown` when the object also has `properties`, or no `additionalProperties`) |
| `oneOf`, `anyOf` | A union; with a `discriminator`, each member is tagged with its value |
| `allOf` | An intersection |
| `$ref` to a component | That schema's type |
| A property in `required` but not in `properties` | `{} \| null` (any value except `undefined`) |
| No schema, or `{}` (also `type: string, format: binary` in OpenAPI 3.0) | `unknown`; `Blob` as a part of a multipart body |

`title`, `description`, `default`, and `deprecated` become JSDoc comments, so
they appear in editor hovers. Formats are not converted; see
[Strict types](./strict-types#formats-are-not-converted).

## Subtypes

Some schemas list their subtypes in `oneOf`, and each subtype extends the
parent with `allOf`. springdoc emits this pattern for Jackson subtypes. Such a
parent schema becomes the union of its subtypes. Each subtype gets the
parent's own properties instead of a reference back to the parent, because
TypeScript cannot resolve that reference.

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
# Cat is defined like Dog, with `lives: { type: integer }`.
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

Checking `kind` narrows an `Animal` to a `Dog` or a `Cat`.

oasis-gen does not support a `discriminator` on a parent that has no `oneOf`
and is referenced only by its subtypes through `allOf`. Such a parent stays a
plain object type.

## Type aliases and `components`

Each schema is defined as a type alias with a `$` prefix, such as `$Pet`, and
`$Pet$Request` for a request variant. Hovers and error messages show these
names. The exported names, `schemas`, and `components` all refer to these
aliases:

| Type | Description |
| --- | --- |
| `schemas["Pet"]`, or `components["schemas"]["Pet"]` | The same type as `Pet` |
| `components["requestSchemas"]` | Request variants of component schemas |
