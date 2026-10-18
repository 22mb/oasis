// Type-level tests for tests/fixtures/features.yaml: compiled by `tsc`, never run.
import { createClient, type ApiResponse, type components, type schemas } from "../golden/features.ts";
import type { Equal, Expect } from "./equal.ts";

// readOnly and writeOnly give requests and responses different shapes.
export type Exact = [
  Expect<Equal<schemas["Account"], { id: number; name: string; password?: string; owner?: schemas["Owner"] }>>,
  Expect<Equal<
    components["requestSchemas"]["Account"],
    { id?: number; name: string; password: string; owner?: components["requestSchemas"]["Owner"] }
  >>,
  // Recursion through an array or index signature at the top of a schema.
  Expect<Equal<schemas["Json"], string | number | boolean | null | schemas["Json"][] | { [key: string]: schemas["Json"] }>>,
  // A parent listing its subtypes in `oneOf` while they extend it with `allOf`: the parent
  // is the union, and each subtype gets the parent's own part instead of a reference back.
  Expect<Equal<schemas["Animal"], { kind: string } & ((schemas["Dog"] & { kind: "Dog" }) | (schemas["Cat"] & { kind: "Cat" }))>>,
  Expect<Equal<schemas["Dog"], { kind: string } & { bark?: boolean }>>,
  // YAML 1.2: `yes`, `off`, `N` stay strings; `null` in a typed enum stays `null`.
  Expect<Equal<schemas["Flag"], "N" | "Y" | "yes" | "off" | "A">>,
  Expect<Equal<schemas["Level"], 1 | 2 | null>>,
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

// `default` may be a success, so it survives narrowing on `ok`.
export function defaultMayBeOk(res: ApiResponse<{ "404": number; default: string }>) {
  if (res.ok) res.data satisfies string;
}
