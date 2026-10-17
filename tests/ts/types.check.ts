// Type-level tests for the generated client: compiled by `tsc`, never run.
import { createClient, type RequestOptions, type schemas } from "../golden/petstore.ts";
import type { Equal, Expect } from "./equal.ts";

const api = createClient();

// Exact shapes, so that a type widening (e.g. to `unknown`) or a required input
// becoming optional fails here.
type Result<S, O, D> = { status: S; ok: O; data: D; response: Response };
export type Exact = [
  Expect<Equal<schemas["NewPet"], { name: string; kind: "cat" | "dog"; tag?: string | null }>>,
  Expect<Equal<schemas["Pet"], schemas["NewPet"] & { id: number; owner?: schemas["Owner"] }>>,
  Expect<Equal<schemas["Owner"], { type: "person"; name: string } | { type: "shelter"; id: number }>>,
  Expect<Equal<schemas["Error"], { title: string; detail?: string; [key: string]: unknown }>>,
  Expect<Equal<schemas["Point"], [number, number]>>,
  Expect<Equal<Parameters<typeof api.GET<"/pets/{petId}">>, [path: "/pets/{petId}", init: { path: { petId: number } } & RequestOptions]>>,
  Expect<Equal<Parameters<typeof api.DELETE<"/pets/{petId}">>, [path: "/pets/{petId}", init: { path: { petId: number } } & RequestOptions]>>,
  Expect<Equal<
    Parameters<typeof api.POST<"/pets">>,
    [path: "/pets", init: (
      | { contentType?: "application/json"; body: schemas["NewPet"] }
      | { contentType: "application/x-www-form-urlencoded"; body: schemas["NewPet"] }
    ) & RequestOptions]
  >>,
  Expect<Equal<
    Awaited<ReturnType<typeof api.GET<"/pets/{petId}">>>,
    Result<200, true, schemas["Pet"]> | Result<404, false, schemas["Error"]>
  >>,
  Expect<Equal<
    Awaited<ReturnType<typeof api.GET<"/pets">>>,
    Result<200, true, schemas["Pet"][]> | Result<number, boolean, schemas["Error"]>
  >>,
  Expect<Equal<Awaited<ReturnType<typeof api.DELETE<"/pets/{petId}">>>, Result<204, true, undefined>>>,
  // No required inputs: init is optional.
  Expect<Equal<Parameters<typeof api.QUERY<"/pets">>, [
    path: "/pets",
    init?: { contentType?: "application/json"; body?: { name?: string } } & RequestOptions,
  ]>>,
];

export async function checks() {
  // Path parameters are required and typed.
  const pet = await api.GET("/pets/{petId}", { path: { petId: 1 } });
  // @ts-expect-error missing init with required path params
  await api.GET("/pets/{petId}");
  // @ts-expect-error wrong path param type
  await api.GET("/pets/{petId}", { path: { petId: "1" } });

  // Results narrow by `ok` and status.
  if (pet.ok) {
    const p: schemas["Pet"] = pet.data;
    const id: number = p.id;
    void id;
  } else {
    const title: string = pet.data.title;
    void title;
  }
  if (pet.status === 404) {
    const e: schemas["Error"] = pet.data;
    void e;
  }

  // Operations without required inputs take no init.
  await api.GET("/pets");
  await api.GET("/pets", { query: { limit: 10, ids: [1, 2], filter: { kind: "cat" } }, signal: AbortSignal.timeout(1000) });
  // @ts-expect-error unknown enum value
  await api.GET("/pets", { query: { filter: { kind: "bird" } } });

  // Request bodies: JSON by default, other media types need an explicit contentType.
  await api.POST("/pets", { body: { name: "Tama", kind: "cat" } });
  await api.POST("/pets", { contentType: "application/x-www-form-urlencoded", body: { name: "Tama", kind: "cat" } });
  // @ts-expect-error body is required
  await api.POST("/pets", {});
  // @ts-expect-error unknown method for this path
  await api.PATCH("/pets", {});
  // @ts-expect-error unknown path
  await api.GET("/nope");
  // @ts-expect-error paths are templates; values go in `path`
  await api.GET("/pets/123");

  // QUERY and additionalOperations methods.
  const found = await api.QUERY("/pets", { body: { name: "Tama" } });
  if (found.ok) found.data satisfies schemas["Pet"][];
  await api.LINK("/pets/{petId}", { path: { petId: 1 }, header: { Link: "</owners/1>" } });

  // querystring parameter.
  await api.GET("/search", { querystring: { term: "cat" } });
  // @ts-expect-error querystring is required
  await api.GET("/search");

  // Binary and streaming responses.
  const photo = await api.GET("/pets/{petId}/photo", { path: { petId: 1 } });
  if (photo.ok) photo.data satisfies Blob;
  const feed = await api.GET("/stream/jsonl");
  if (feed.ok) for await (const p of feed.data) p satisfies schemas["Pet"];
  const sse = await api.GET("/stream/sse");
  if (sse.ok) for await (const e of sse.data) e.data satisfies string;
}
