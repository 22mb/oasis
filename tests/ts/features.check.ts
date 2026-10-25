// Type-level tests for tests/fixtures/features.yaml: compiled by `tsc`, never run.
import { createClient, type ApiResponse, type components, type schemas } from "../golden/features.ts";
import type { Codes, Equal, Expect, Failure, Result, Success } from "./equal.ts";

// An intersection as one object type, to compare its resulting properties.
type Flat<T> = { [K in keyof T]: T[K] };

// readOnly properties are absent from requests and writeOnly ones from responses.
export type Exact = [
  Expect<Equal<schemas["Account"], { id: number; name: string; password?: never; owner?: schemas["Owner"] }>>,
  Expect<Equal<
    components["requestSchemas"]["Account"],
    { id?: never; name: string; password: string; owner?: components["requestSchemas"]["Owner"] }
  >>,
  // ... also when they come from a referenced schema or another allOf member.
  Expect<Equal<Flat<schemas["Entry"]>, { serial: number; secret?: never; label: string }>>,
  Expect<Equal<Flat<components["requestSchemas"]["Entry"]>, { serial?: never; secret: string; label: string }>>,
  Expect<Equal<schemas["Holder"], { serial: number }>>,
  Expect<Equal<components["requestSchemas"]["Holder"], { serial?: never }>>,
  // Recursion through an array or index signature at the top of a schema.
  Expect<Equal<schemas["Json"], string | number | boolean | null | schemas["Json"][] | { [key: string]: schemas["Json"] }>>,
  // A parent listing its subtypes in `oneOf` while they extend it with `allOf`: the parent
  // is the union, and each subtype gets the parent's own part instead of a reference back.
  Expect<Equal<schemas["Animal"], { kind: string } & ((schemas["Dog"] & { kind: "Dog" }) | (schemas["Cat"] & { kind: "Cat" }))>>,
  Expect<Equal<schemas["Dog"], { kind: string } & { bark?: boolean }>>,
  // YAML 1.2: `yes`, `off`, `N` stay strings; `null` in a typed enum stays `null`.
  Expect<Equal<schemas["Flag"], "N" | "Y" | "yes" | "off" | "A">>,
  Expect<Equal<schemas["Level"], 1 | 2 | null>>,
  // A union of one variant keeps its parentheses next to `[]` and `&`.
  Expect<Equal<schemas["Tags"], ("a" | "b")[]>>,
  Expect<Equal<schemas["Tagged"], schemas["Base"] & (schemas["Circle"] | schemas["Square"])>>,
  // `defaultMapping`: the default variant takes every value the mapping does not list.
  // Explicitly mapped schemas keep their implicit name as a value too.
  Expect<Equal<
    schemas["Vehicle"],
    | (schemas["Car"] & { kind: "car" | "Car" })
    | (schemas["Bike"] & { kind: "Bike" })
    | (schemas["OtherVehicle"] & { kind: string })
  >>,
];

const api = createClient();

export async function checks() {
  // readOnly properties are optional in requests; writeOnly ones are required there.
  await api.POST("/upload", { body: { name: "a", password: "p", owner: {} } });
  // @ts-expect-error writeOnly `password` is required in requests
  await api.POST("/upload", { body: { name: "a" } });
  const created = await api.POST("/upload", { body: { name: "a", password: "p" } });
  if (created.status === 201) {
    const id: number = created.data.id;
    // @ts-expect-error writeOnly `password` is optional in responses
    const password: string = created.data.password;
    void id, password;
  }

  // `required` next to `allOf` makes the inherited property required.
  const withId: schemas["WithId"] = { id: 1, note: "x" };
  const id: number = withId.id;
  // @ts-expect-error `id` is required
  const missing: schemas["WithId"] = { note: "x" };
  void id, missing;

  // Discriminated unions narrow on the mapped values.
  const shape = {} as schemas["Shape"];
  if (shape.kind === "c") shape.r satisfies number;

  // Recursive schemas behind non-component $refs.
  const tree = {} as schemas["Tree"];
  tree.left?.right?.value satisfies number | undefined;

  // Subtypes narrow on the discriminator and stand on their own.
  const animal = {} as schemas["Animal"];
  if (animal.kind === "Dog") animal.bark satisfies boolean | undefined;
  const dog: schemas["Dog"] = { kind: "Dog", bark: true };
  const json: schemas["Json"] = [1, "a", { nested: [null, true] }];
  void dog, json;

  // Media type ranges decode to `unknown`.
  const item = await api.GET("/items/{coords}", { path: { coords: { x: 1 } } });
  // @ts-expect-error `*/*` is not known to be a string
  const text: string = item.data;
  void text;
}

// `default` covers undeclared successes too, so it survives narrowing on `ok`.
export function defaultMayBeOk(res: ApiResponse<{ "404": number; default: string }>) {
  if (res.ok) res.data satisfies string;
}


// Each response key covers its own statuses: a range minus the codes declared in
// it, `default` the rest, and statuses nothing covers have an unknown body.
export type Statuses = [
  Expect<Equal<
    Awaited<ReturnType<typeof api.GET<"/shapes">>>,
    | Result<200, true, schemas["Shape"]>
    | Result<301, false, undefined>
    | Result<404, false, undefined>
    | Result<Exclude<Codes<400, 500>, 404>, false, string>
    | Result<Exclude<Success, 200>, true, unknown>
    | Result<Exclude<Failure, 301 | Codes<400, 500>>, false, unknown>
  >>,
  // With `2XX` declared, no success is left for `default`.
  Expect<Equal<ApiResponse<{ "2XX": number; default: string }>, Result<Success, true, number> | Result<Failure, false, string>>>,
  // `default` alone covers every status.
  Expect<Equal<ApiResponse<{ default: string }>, Result<Success, true, string> | Result<Failure, false, string>>>,
];

export async function access() {
  await api.POST("/upload", { body: { name: "a", password: "p" } });
  // @ts-expect-error readOnly properties are not sent
  await api.POST("/upload", { body: { id: 1, name: "a", password: "p" } });
  const res = await api.POST("/upload", { body: { name: "a", password: "p" } });
  // @ts-expect-error writeOnly properties are not received
  if (res.status === 201) res.data.password satisfies string;
}
