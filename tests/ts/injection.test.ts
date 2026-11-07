import { expect, test } from "vitest";

test("document text cannot run code in the generated file", async () => {
  await import("../golden/injection.ts");
  expect((globalThis as { __oasis_pwned?: unknown }).__oasis_pwned).toBe(undefined);
});
