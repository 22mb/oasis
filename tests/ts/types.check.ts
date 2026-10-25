// Type-level tests for the generated client: compiled by `tsc`, never run.
import { createClient, type RequestOptions, type schemas } from "../golden/petstore.ts";
import type { Equal, Expect, Failure, Result, Success } from "./equal.ts";

const api = createClient();

// Exact shapes, so that a type widening (e.g. to `unknown`) or a required input
// becoming optional fails here.
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
    | Result<200, true, schemas["Pet"]>
    | Result<404, false, schemas["Error"]>
    // Undeclared statuses remain possible.
    | Result<Exclude<Success, 200>, true, unknown>
    | Result<Exclude<Failure, 404>, false, unknown>
  >>,
  Expect<Equal<
    Awaited<ReturnType<typeof api.GET<"/pets">>>,
    // `default` covers every other status, successes included.
    | Result<200, true, schemas["Pet"][]>
    | Result<Exclude<Success, 200>, true, schemas["Error"]>
    | Result<Failure, false, schemas["Error"]>
  >>,
  Expect<Equal<
    Awaited<ReturnType<typeof api.DELETE<"/pets/{petId}">>>,
    Result<204, true, undefined> | Result<Exclude<Success, 204>, true, unknown> | Result<Failure, false, unknown>
  >>,
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

  // Results narrow by status. `ok` is `response.ok`, which an undeclared 2xx
  // also has, so it does not narrow to the declared success.
  if (pet.status === 200) {
    const p: schemas["Pet"] = pet.data;
    const id: number = p.id;
    void id;
  }
  if (pet.ok) {
    // @ts-expect-error the body of an undeclared success is unknown
    const p: schemas["Pet"] = pet.data;
    void p;
  }
  // @ts-expect-error not every failure is the declared 404
  if (!pet.ok) pet.data satisfies schemas["Error"];
  // Undeclared statuses can be compared with.
  if (pet.status === 500) pet.data satisfies unknown;
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
  if (found.status === 200) found.data satisfies schemas["Pet"][];
  await api.LINK("/pets/{petId}", { path: { petId: 1 }, header: { Link: "</owners/1>" } });

  // querystring parameter.
  await api.GET("/search", { querystring: { term: "cat" } });
  // @ts-expect-error querystring is required
  await api.GET("/search");

  // Binary and streaming responses.
  const photo = await api.GET("/pets/{petId}/photo", { path: { petId: 1 } });
  if (photo.status === 200) photo.data satisfies Blob;
  const feed = await api.GET("/stream/jsonl");
  if (feed.status === 200) for await (const p of feed.data) p satisfies schemas["Pet"];
  const sse = await api.GET("/stream/sse");
  if (sse.status === 200) for await (const e of sse.data) e.data satisfies string;
}
