// Type-level tests for tests/fixtures/security.yaml: compiled by `tsc`, never run.
import { auth, type SecuritySchemeName } from "../golden/security.ts";

export const names: SecuritySchemeName[] = ["bearer", "basic", "apiKey", "queryKey", "session", "oauth"];
// @ts-expect-error unknown security scheme
export const typo = auth({ bearr: "x" });
export const async = auth({ bearer: async (scopes) => scopes.join(" ") });
