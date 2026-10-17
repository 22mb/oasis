import assert from "node:assert/strict";
import { test } from "node:test";

test("document text cannot run code in the generated file", async () => {
  await import("../golden/injection.ts");
  assert.equal((globalThis as { __oasis_pwned?: unknown }).__oasis_pwned, undefined);
});
