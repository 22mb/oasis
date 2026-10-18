// Type-level tests for tests/fixtures/v30.yaml (OpenAPI 3.0): compiled by `tsc`, never run.
import type { schemas } from "../golden/v30.ts";
import type { Equal, Expect } from "./equal.ts";

// `nullable: true` adds `null`: to a type, to an enum, to a `$ref` (bare or in
// `allOf`), to an inline object, and to an array and its items. `nullable: false`
// adds nothing.
export type Exact = [
  Expect<Equal<schemas["Kind"], "cat" | "dog" | null>>,
  Expect<Equal<
    schemas["Shape"],
    (schemas["Circle"] & { kind: "Circle" }) | (schemas["Square"] & { kind: "Square" }) | null
  >>,
  Expect<Equal<schemas["Pet"], {
    id: number;
    name: string | null;
    kind: schemas["Kind"];
    owner?: schemas["Owner"] | null;
    home?: schemas["Owner"] | null;
    tags: (string | null)[] | null;
    address?: { street?: string } | null;
    shape?: schemas["Shape"];
    weight?: number;
  }>>,
];
