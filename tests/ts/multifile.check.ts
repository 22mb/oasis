// Type-level tests for tests/fixtures/multifile (a document split across files):
// compiled by `tsc`, never run.
import { createClient, type RequestOptions, type components, type schemas } from "../golden/multifile.ts";
import type { Equal, Expect, Failure, Result, Success } from "./equal.ts";

type Refs = components["refs"];
type Pet = Refs["schemas/pet.yaml#"];
type Owner = Refs["schemas/owner.yaml#"];

const api = createClient();

// References across files, back to the referring file, and to the file itself.
export type Exact = [
  Expect<Equal<schemas["Pet"], Pet>>,
  Expect<Equal<Pet, { id: number; name: string; tags?: Refs["schemas/pet.yaml#/$defs/Tags"]; owner?: Owner }>>,
  Expect<Equal<Refs["schemas/pet.yaml#/$defs/Tags"], string[]>>,
  Expect<Equal<Owner, { id: number; pets?: Pet[]; partner?: Owner }>>,
  // A schema found by its `$id`, and a parameter by an `$anchor` in another file.
  Expect<Equal<schemas["Error"], { title: string }>>,
  Expect<Equal<Parameters<typeof api.GET<"/pets">>, [path: "/pets", init?: { query?: { limit?: number } } & RequestOptions]>>,
  Expect<Equal<
    Awaited<ReturnType<typeof api.GET<"/owners/{ownerId}">>>,
    | Result<200, true, Owner>
    | Result<Exclude<Success, 200>, true, schemas["Error"]>
    | Result<Failure, false, schemas["Error"]>
  >>,
];
