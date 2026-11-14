import { readFileSync } from "node:fs";
import { defineConfig } from "@rspress/core";
import { pluginTwoslash } from "@rspress/plugin-twoslash";
import { twoslashQueriesAsLines } from "./twoslash-queries.ts";

const repository = "https://github.com/22mb/oasis";

// `ts twoslash` code blocks type-check against clients generated from the test
// fixtures, which `cargo test` keeps current. `./api.ts` is the petstore
// client, and a block can import `./features.ts` or `./security.ts` before
// `// ---cut---`.
const golden = (name: string) =>
  readFileSync(new URL(`../tests/golden/${name}.ts`, import.meta.url), "utf8");

/** Every table has one row of column headers. */
function columnHeaders() {
  type Node = { type: string; tagName?: string; properties?: Record<string, unknown>; children?: Node[] };
  const visit = (node: Node) => {
    if (node.tagName === "th") node.properties = { ...node.properties, scope: "col" };
    node.children?.forEach(visit);
  };
  return visit;
}

export default defineConfig({
  // Served from GitHub Pages at https://22mb.github.io/oasis/.
  siteOrigin: "https://22mb.github.io",
  base: "/oasis/",
  title: "oasis-gen",
  description: "A strictly typed API client generator for OpenAPI, with zero runtime dependencies",
  icon: new URL("./public/logo.svg", import.meta.url),
  logo: "/logo.svg",
  logoText: "oasis-gen",
  // `head` paths are not prefixed with `base`.
  head: [
    ["link", { rel: "apple-touch-icon", href: "/oasis/logo-512.png" }],
    ["meta", { name: "theme-color", content: "#0B1F26" }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:image", content: "https://22mb.github.io/oasis/ogp.png" }],
    ["meta", { name: "twitter:card", content: "summary_large_image" }],
  ],
  // The README links to the images in `site/public`, so they stay there
  // instead of moving under `docs`.
  builderConfig: {
    server: { publicDir: { name: "public" } },
    // `import text from "./file?raw"` gives the file's text.
    tools: { rspack: { module: { rules: [{ resourceQuery: /^\?raw$/, type: "asset/source" }] } } },
  },
  markdown: {
    rehypePlugins: [columnHeaders],
    shiki: { transformers: [twoslashQueriesAsLines] },
  },
  plugins: [
    pluginTwoslash({
      twoslashOptions: {
        extraFiles: {
          "api.ts": golden("petstore"),
          "features.ts": golden("features"),
          "security.ts": golden("security"),
        },
        compilerOptions: {
          allowImportingTsExtensions: true,
          noEmit: true,
          exactOptionalPropertyTypes: true,
          noUncheckedIndexedAccess: true,
        },
      },
    }),
  ],
  themeConfig: {
    // Disable the "Copy Markdown" button on pages because llms.txt
    // serves that purpose.
    llmsUI: false,
    nav: [
      {
        text: "Guide",
        link: "/getting-started",
        activeMatch: "^/(getting-started|strict-types|client|middleware|authentication|recipes)$",
      },
      { text: "Reference", link: "/reference", activeMatch: "^/(reference|schemas|openapi-support)$" },
      { text: "Playground", link: "/playground" },
    ],
    sidebar: {
      "/": [
        {
          text: "Introduction",
          items: [
            { text: "Getting started", link: "/getting-started" },
            { text: "Strict types", link: "/strict-types" },
          ],
        },
        {
          text: "Guide",
          items: [
            { text: "Calling operations", link: "/client" },
            { text: "Middleware", link: "/middleware" },
            { text: "Authentication", link: "/authentication" },
            { text: "Recipes", link: "/recipes" },
          ],
        },
        {
          text: "Reference",
          items: [
            { text: "CLI and API", link: "/reference" },
            { text: "Schemas", link: "/schemas" },
            { text: "OpenAPI support", link: "/openapi-support" },
          ],
        },
      ],
    },
    socialLinks: [{ icon: "github", mode: "link", content: repository }],
  },
});
