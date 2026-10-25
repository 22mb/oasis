// Type-level tests for tests/fixtures/multifile (a document split across files):
// compiled by `tsc`, never run.
import { createClient, type RequestOptions, type schemas } from "../golden/multifile.ts";
import type { Equal, Expect, Failure, Result, Success } from "./equal.ts";

// The schemas in other files are reached through the component that refers to them.
type Pet = schemas["Pet"];
type Owner = NonNullable<Pet["owner"]>;

const api = createClient();

// References across files, back to the referring file, and to the file itself.
export type Exact = [
  Expect<Equal<Pet, { id: number; name: string; tags?: string[]; owner?: Owner }>>,
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
