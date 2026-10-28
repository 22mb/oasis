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
// A value that may be missing, such as an environment variable, is accepted as it is.
declare const env: Record<string, string | undefined>;
export const fromEnv = auth({ apiKey: env["API_KEY"], bearer: undefined });
