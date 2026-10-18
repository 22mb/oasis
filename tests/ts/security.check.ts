// Type-level tests for tests/fixtures/security.yaml: compiled by `tsc`, never run.
import { auth, type SecuritySchemeName } from "../golden/security.ts";
import type { Equal, Expect } from "./equal.ts";

export type Exact = [
  Expect<Equal<
    SecuritySchemeName,
    "bearer" | "basic" | "digest" | "apiKey" | "queryKey" | "session" | "oauth" | "oidc" | "mtls"
  >>,
];
// @ts-expect-error unknown security scheme
export const typo = auth({ bearr: "x" });
export const async = auth({ bearer: async (scopes) => scopes.join(" ") });
