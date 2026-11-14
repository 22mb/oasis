import { readFileSync } from "node:fs";
import { transformerTwoslash } from "@shikijs/vitepress-twoslash";
import { defineConfig } from "vitepress";

const repository = "https://github.com/22mb/oasis";

// `ts twoslash` code blocks type-check against clients generated from the test
// fixtures, which `cargo test` keeps current: `./api.ts` is the petstore one,
// and a block can import `./features.ts` or `./security.ts` before `// ---cut---`.
const golden = (name: string) =>
  readFileSync(new URL(`../../tests/golden/${name}.ts`, import.meta.url), "utf8");

export default defineConfig({
  title: "oasis-gen",
  lang: "en",
  description: "A strictly typed API client generator for OpenAPI, with zero runtime dependencies",
  // `head` paths are not prefixed with `base`, unlike `themeConfig.logo`.
  head: [
    ["link", { rel: "icon", type: "image/svg+xml", href: "/oasis/logo.svg" }],
    ["link", { rel: "apple-touch-icon", href: "/oasis/logo-512.png" }],
    ["meta", { name: "theme-color", content: "#0B1F26" }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:title", content: "oasis-gen" }],
    ["meta", { property: "og:image", content: "https://22mb.github.io/oasis/ogp.png" }],
    ["meta", { name: "twitter:card", content: "summary_large_image" }],
  ],
  // Served from GitHub Pages at https://22mb.github.io/oasis/.
  base: "/oasis/",
  cleanUrls: true,
  lastUpdated: false,
  markdown: {
    // Every table has one row of column headers.
    config(md) {
      md.renderer.rules.th_open = (tokens, idx, options, _env, self) => {
        tokens[idx]!.attrSet("scope", "col");
        return self.renderToken(tokens, idx, options);
      };
    },
    // The default GitHub themes leave some tokens, such as comments, below 4.5:1.
    theme: { light: "github-light-high-contrast", dark: "github-dark-high-contrast" },
    // VitePress loads only the languages of code blocks, but Twoslash also
    // highlights the examples in hovered JSDoc, such as TanStack Query's `tsx`.
    languages: ["tsx"],
    codeTransformers: [
      transformerTwoslash({
        // Show `^?` results as lines of their own instead of popups over the code.
        queryRendering: "line",
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
  },
  themeConfig: {
    logo: { src: "/logo.svg", width: 24, height: 24 },
    search: { provider: "local" },
    nav: [
      { text: "Guide", link: "/getting-started" },
      { text: "Reference", link: "/reference" },
      { text: "Playground", link: "/playground" },
    ],
    sidebar: [
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
    socialLinks: [{ icon: "github", link: repository }],
  },
});
