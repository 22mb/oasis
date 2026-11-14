// Runs the runtime tests that don't need a server in Node.js, Chromium, Firefox
// and WebKit. e2e.test.ts, which starts a server, runs under `node --test`.
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

const include = ["features.test.ts", "security.test.ts", "injection.test.ts"];

export default defineConfig({
  test: {
    projects: [
      { test: { name: "node", include } },
      {
        test: {
          name: "browsers",
          include,
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [{ browser: "chromium" }, { browser: "firefox" }, { browser: "webkit" }],
          },
        },
      },
    ],
  },
});
